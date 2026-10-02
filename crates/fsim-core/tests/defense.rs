//! Оборона без мяча: прессинг, тени прикрытия, опека, контрпрессинг, детекторы роя и дрожания.

use fsim_core::config::{BallPoint, Config, Style};
use fsim_core::data::Position;
use fsim_core::defense::{
    DUTY_COUNTER, DUTY_COVER, DUTY_MARK, DUTY_OWNER, DUTY_PRESS, DUTY_SUPPORT, DUTY_ZONAL,
};
use fsim_core::embedded;
use fsim_core::metrics::players_near_ball;
use fsim_core::positioning::team::TeamSetup;
use fsim_core::positioning::world_to_team;
use fsim_core::scenario_sim::ScenarioSim;
use fsim_core::sink::NoopSink;
use glam::Vec2;

const SLOTS: usize = 11;

fn cfg() -> Config {
    embedded::config().unwrap()
}

fn team(c: &Config, formation: &str, style: &Style) -> TeamSetup {
    TeamSetup::uniform(c, formation, style, c.sandbox.uniform_attr).unwrap()
}

fn sim_styles(c: &Config, name: &str, styles: [&str; 2]) -> ScenarioSim<NoopSink> {
    let sc = &c.scenarios[name];
    let teams = [0, 1].map(|k| team(c, &sc.formations[k], &c.styles[styles[k]]));
    ScenarioSim::new(1, c, sc, teams, NoopSink)
}

fn sim(c: &Config, name: &str) -> ScenarioSim<NoopSink> {
    let sc = &c.scenarios[name];
    sim_styles(c, name, [&sc.styles[0], &sc.styles[1]])
}

fn hz(c: &Config) -> u32 {
    c.physics.time.tick_hz
}

fn run(c: &Config, s: &mut ScenarioSim<NoopSink>, secs: f32) {
    s.run((secs * hz(c) as f32) as u32);
}

/// Среднее число игроков команды в активной обороне и доля времени с включённым прессингом.
fn press_stats(c: &Config, s: &mut ScenarioSim<NoopSink>, team: usize, secs: f32) -> (f32, f32) {
    let (mut sum, mut on, mut n) = (0.0f32, 0u32, 0u32);
    for _ in 0..(secs * hz(c) as f32) as u32 {
        s.step();
        sum += s.pressers(team) as f32;
        on += u32::from(s.plan(team).press_on || s.plan(team).counter_on);
        n += 1;
    }
    (sum / n as f32, on as f32 / n as f32)
}

fn defender(s: &ScenarioSim<NoopSink>) -> usize {
    if s.world.teams[0].has_ball {
        1
    } else {
        0
    }
}

#[test]
fn pressing_players_count_follows_press_intensity() {
    let c = cfg();
    let styles = [
        "gegenpress",
        "positional",
        "balanced",
        "wing_play",
        "direct_long_ball",
        "low_block_counter",
    ];
    for scenario in ["buildup_vs_press", "back_pass_trigger", "possession_flips"] {
        let dur = c.scenarios[scenario].duration_s;
        let avg: Vec<(f32, f32, &str)> = styles
            .iter()
            .map(|n| {
                let mut s = sim_styles(&c, scenario, ["positional", n]);
                (
                    c.styles[*n].press_intensity,
                    press_stats(&c, &mut s, 1, dur).0,
                    *n,
                )
            })
            .collect();
        for a in &avg {
            for b in &avg {
                if a.0 - b.0 >= 0.19 {
                    assert!(
                        a.1 > b.1,
                        "{scenario}: {} (press {}) давит {:.2} игрока, {} (press {}) давит {:.2}",
                        a.2,
                        a.0,
                        a.1,
                        b.2,
                        b.0,
                        b.1
                    );
                }
            }
        }
        let by = |n: &str| avg.iter().find(|r| r.2 == n).unwrap().1;
        assert!(
            by("gegenpress") >= 2.5,
            "{scenario}: высокий прессинг {:.2}",
            by("gegenpress")
        );
        assert!(
            by("low_block_counter") <= 0.5,
            "{scenario}: низкий блок {:.2}",
            by("low_block_counter")
        );
    }
}

#[test]
fn mid_intensity_presses_only_after_a_trigger_and_then_stops() {
    let c = cfg();
    let mut s = sim(&c, "back_pass_trigger"); // обе команды balanced: intensity 0,5 ниже порога постоянного прессинга
    let on_at = |secs: f32, s: &mut ScenarioSim<NoopSink>| {
        let target = (secs * hz(&c) as f32) as u32;
        while s.world.tick < target {
            s.step();
        }
        s.plan(1).press_on
    };
    assert!(!on_at(12.0, &mut s), "до паса назад прессинга нет");
    assert!(on_at(19.0, &mut s), "после паса назад прессинг включён");
    assert!(
        !on_at(26.0, &mut s),
        "триггер отыгран, команда возвращается в блок"
    );
}

#[test]
fn high_press_style_presses_continuously_in_its_zone() {
    let c = cfg();
    let mut s = sim_styles(&c, "possession_flips", ["balanced", "gegenpress"]);
    // Мяч в средней трети, зона прессинга высокого блока покрывает его всё время.
    let (_, on) = press_stats(&c, &mut s, 1, 24.0);
    assert!(on > 0.95, "доля прессинга {on:.2}");
}

#[test]
fn counter_press_starts_at_once_lasts_3_to_6_seconds_and_ends() {
    let c = cfg();
    let mut s = sim(&c, "counter_press_loss"); // A gegenpress теряет мяч на 10-й секунде
    run(&c, &mut s, 9.9);
    assert!(!s.plan(0).counter_on);
    run(&c, &mut s, 0.6); // 10,5 с
    assert!(s.plan(0).counter_on, "контрпрессинг сразу после потери");
    let busy = s.pressers(0);
    assert!(
        (3..=6).contains(&busy),
        "в контрпрессинге {busy} игроков, ожидается 3–5 (до 6 со страховкой)"
    );
    // Через 3 секунды ближе 12 м к мячу уже минимум трое игроков A.
    run(&c, &mut s, 3.0);
    let near = (1..SLOTS)
        .filter(|&i| {
            let p = &s.world.players;
            Vec2::new(
                p.pos_x[i] - s.world.ball_pos.x,
                p.pos_y[i] - s.world.ball_pos.y,
            )
            .length()
                < 12.0
        })
        .count();
    assert!(near >= 3, "у мяча только {near} игроков A");
    // После заданной длительности (не больше 6 с) контрпрессинг заканчивается.
    run(&c, &mut s, 3.5); // 17 с
    assert!(
        !s.plan(0).counter_on,
        "контрпрессинг должен кончиться не позже 6 с"
    );
}

#[test]
fn style_without_counter_press_does_not_counter_press() {
    let c = cfg();
    // B (low_block_counter) теряет мяч позже, когда владение возвращается к A.
    let mut sc = c.scenarios["counter_press_loss"].clone();
    sc.possession = vec![
        fsim_core::config::PossessionChange { t: 0.0, team: 1 },
        fsim_core::config::PossessionChange { t: 10.0, team: 0 },
    ];
    let teams = [0, 1].map(|k| team(&c, &sc.formations[k], &c.styles[&sc.styles[k]]));
    let mut s = ScenarioSim::new(1, &c, &sc, teams, NoopSink);
    let hz = hz(&c);
    for _ in 0..(14.0 * hz as f32) as u32 {
        s.step();
        assert!(
            !s.plan(1).counter_on,
            "low_block_counter не контрпрессингует"
        );
    }
}

/// Инварианты назначений на каждом тике, по разным стилям и сценариям.
#[test]
fn assignments_are_valid_every_tick() {
    let c = cfg();
    let pairs = [
        ("positional", "gegenpress"),
        ("gegenpress", "low_block_counter"),
        ("wing_play", "direct_long_ball"),
        ("balanced", "balanced"),
    ];
    for scenario in c.scenarios.keys() {
        for (a, b) in pairs {
            let mut s = sim_styles(&c, scenario, [a, b]);
            let dur = c.scenarios[scenario].duration_s;
            for _ in 0..(dur * hz(&c) as f32) as u32 {
                s.step();
                let d = defender(&s);
                let st = &c.styles[[a, b][d]];
                let plan = s.plan(d);
                let mut zonal = [false; SLOTS];
                let (mut press, mut support, mut cover, mut mark, mut counter) = (0, 0, 0, 0, 0);
                for slot in 1..SLOTS {
                    match plan.kind[slot] {
                        DUTY_ZONAL => {
                            let t = s.world.players.anchor_x[d * SLOTS + slot];
                            assert!(t.is_finite());
                        }
                        DUTY_PRESS => press += 1,
                        DUTY_SUPPORT => support += 1,
                        DUTY_COVER => cover += 1,
                        DUTY_MARK => mark += 1,
                        DUTY_COUNTER => counter += 1,
                        k => panic!("{scenario}: слот {slot} с обязанностью {k}"),
                    }
                    if matches!(
                        plan.kind[slot],
                        DUTY_PRESS | DUTY_SUPPORT | DUTY_COVER | DUTY_COUNTER
                    ) && s.team(d).slots[slot].pos == Position::CB
                    {
                        assert!(
                            st.press_intensity >= c.defense.assign.cb_press_threshold,
                            "{scenario}: центральный защитник вышел в прессинг при intensity {}",
                            st.press_intensity
                        );
                    }
                }
                // Зональные места не повторяются.
                let zones = s.zonal_slots(d);
                for z in zones.into_iter().flatten() {
                    assert!(
                        !zonal[z as usize],
                        "{scenario}: зональное место {z} занято дважды"
                    );
                    zonal[z as usize] = true;
                }
                assert!(press <= 1, "{scenario}: прессингующих {press}");
                assert!(cover <= 1);
                assert!(support <= c.defense.press.max_support as usize);
                assert!(counter < c.defense.counter_press.players_max as usize);
                assert!(mark <= c.defense.marking.max_marks as usize);
                // Первый слот остаётся вратарём без обязанностей.
                assert_eq!(plan.kind[0], 0);
            }
        }
    }
}

/// Якорь считается по позициям начала тика, а проверка по позициям конца: допуск на движение
/// двух игроков за тик (до 0,5 м каждый).
const LANE_TOLERANCE_M: f32 = 1.0;

fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[test]
fn support_players_stand_on_passing_lanes_from_the_owner() {
    let c = cfg();
    let mut s = sim_styles(&c, "buildup_vs_press", ["positional", "gegenpress"]);
    let mut checked = 0;
    for _ in 0..(30.0 * hz(&c) as f32) as u32 {
        s.step();
        let d = 1usize;
        let a = 0usize;
        let p = &s.world.players;
        let owner = (0..SLOTS).find(|&i| p.duty[a * SLOTS + i] == DUTY_OWNER);
        let Some(owner) = owner else { continue };
        let o = Vec2::new(p.pos_x[owner], p.pos_y[owner]);
        for slot in 1..SLOTS {
            let n = d * SLOTS + slot;
            if p.duty[n] != DUTY_SUPPORT {
                continue;
            }
            let anchor = Vec2::new(p.anchor_x[n], p.anchor_y[n]);
            // Якорь тени лежит на линии паса от владельца к одному из его партнёров.
            let on_lane = (0..SLOTS).filter(|&i| i != owner).any(|i| {
                point_segment_distance(anchor, o, Vec2::new(p.pos_x[i], p.pos_y[i]))
                    < LANE_TOLERANCE_M
            });
            assert!(on_lane, "тень прикрытия вне линий паса");
            checked += 1;
        }
    }
    assert!(
        checked > 50,
        "проверено лишь {checked} тиков с тенями прикрытия"
    );
}

#[test]
fn markers_stay_goal_side_of_their_opponents() {
    let c = cfg();
    let mut style = c.styles["balanced"].clone();
    style.marking = 1.0;
    style.press_intensity = 0.0;
    style.press_triggers.clear();
    let mut sc = c.scenarios["shape_hold"].clone();
    sc.possession.truncate(1);
    sc.duration_s = 30.0;
    sc.ball = vec![
        BallPoint {
            t: 0.0,
            x: 30.0,
            y: 0.0,
            z: 0.0,
        },
        BallPoint {
            t: 30.0,
            x: 30.0,
            y: 0.0,
            z: 0.0,
        },
    ];
    let teams = [
        team(&c, "4-3-3", &c.styles["balanced"]),
        team(&c, "4-3-3", &style),
    ];
    let mut s = ScenarioSim::new(1, &c, &sc, teams, NoopSink);
    run(&c, &mut s, 25.0);
    let half = c.physics.pitch.length_m * 0.5;
    let p = &s.world.players;
    let mut marks = 0;
    for slot in 1..SLOTS {
        let n = SLOTS + slot;
        if p.duty[n] != DUTY_MARK {
            continue;
        }
        marks += 1;
        let opp = p.duty_ref[n] as usize;
        let me = world_to_team(1, Vec2::new(p.pos_x[n], p.pos_y[n]), half);
        let them = world_to_team(1, Vec2::new(p.pos_x[opp], p.pos_y[opp]), half);
        assert!(
            me.x < them.x,
            "опекун не между соперником и своими воротами: {me} против {them}"
        );
        assert!(
            me.distance(them) < 4.0,
            "опекун далеко от подопечного: {:.1} м",
            me.distance(them)
        );
    }
    assert_eq!(
        marks, c.defense.marking.max_marks as usize,
        "число опекунов при marking = 1"
    );
}

#[test]
fn zonal_marking_has_no_marks() {
    let c = cfg();
    let mut style = c.styles["balanced"].clone();
    style.marking = 0.0;
    let sc = &c.scenarios["buildup_vs_press"];
    let teams = [
        team(&c, "4-3-3", &c.styles["positional"]),
        team(&c, "4-3-3", &style),
    ];
    let mut s = ScenarioSim::new(1, &c, sc, teams, NoopSink);
    for _ in 0..(20.0 * hz(&c) as f32) as u32 {
        s.step();
        assert!(s.plan(1).kind.iter().all(|&k| k != DUTY_MARK));
    }
}

/// Детектор роя (`docs/07-validation.md` §9): среднее число полевых игроков ближе 10 м к мячу.
/// Пороги предварительные, уточняются в M7.
const MAX_AVG_NEAR_BALL: f32 = 5.5;
const MAX_NEAR_BALL: usize = 9;

#[test]
fn no_swarm_at_the_ball_in_any_scenario() {
    let c = cfg();
    for name in c.scenarios.keys() {
        let mut s = sim(&c, name);
        let (mut sum, mut mx, mut n) = (0.0f32, 0usize, 0u32);
        for _ in 0..(c.scenarios[name].duration_s * hz(&c) as f32) as u32 {
            s.step();
            let k = players_near_ball(&s.world, 10.0);
            sum += k as f32;
            mx = mx.max(k);
            n += 1;
        }
        let avg = sum / n as f32;
        println!("{name}: около мяча в среднем {avg:.2}, максимум {mx}");
        assert!(
            avg <= MAX_AVG_NEAR_BALL,
            "{name}: рой у мяча, в среднем {avg:.2}"
        );
        assert!(mx <= MAX_NEAR_BALL, "{name}: максимум {mx} игроков у мяча");
    }
}

/// Владелец (ближайший к мячу игрок владеющей команды) не дребезжит: возврат к прежнему
/// владельцу быстрее чем за секунду недопустим. Смены при быстрых передачах по скрипту законны.
#[test]
fn owner_does_not_flip_flop() {
    let c = cfg();
    for name in c.scenarios.keys() {
        let mut s = sim(&c, name);
        let dur = c.scenarios[name].duration_s;
        let hz = hz(&c) as f32;
        let (mut cur, mut before, mut changed_at) = (None, None, 0.0f32);
        let mut flips = 0;
        for tick in 0..(dur * hz) as u32 {
            s.step();
            let p = &s.world.players;
            let now = (0..22).find(|&i| p.duty[i] == DUTY_OWNER);
            if now != cur {
                let t = tick as f32 / hz;
                if now.is_some() && now == before && t - changed_at < 1.0 {
                    flips += 1;
                }
                before = cur;
                cur = now;
                changed_at = t;
            }
        }
        assert_eq!(
            flips, 0,
            "{name}: владелец возвращался к прежнему быстрее секунды {flips} раз"
        );
    }
}

/// Передача опеки: при слабой коммуникации линии игрок дольше держит прежнего подопечного.
/// Подопечный сдвигается вбок: смотрим, при каком сдвиге опекун меняется.
#[test]
fn low_communication_hands_off_marks_later() {
    use fsim_core::defense::{DefenseState, Inputs, Plan};
    let c = cfg();
    let mut style = c.styles["balanced"].clone();
    style.marking = 0.25; // один опекун
    style.press_intensity = 0.0;
    style.press_triggers.clear();
    let slots = &c.formations.0["4-4-2"];
    let len = c.physics.pitch.length_m;
    let base: [Vec2; SLOTS] = std::array::from_fn(|i| {
        Vec2::new(
            slots[i].x * len,
            (slots[i].y - 0.5) * c.physics.pitch.width_m,
        )
    });
    let attackers = team(&c, "4-4-2", &c.styles["balanced"]);

    let marker_after_shift = |communication: f32, shift: f32| -> usize {
        let mut t = team(&c, "4-4-2", &style);
        t.cohesion = communication;
        t.teamwork = [communication; SLOTS];
        let mut att = [Vec2::new(90.0, 0.0); SLOTS];
        let target_att = 9;
        let mut state = DefenseState::new();
        let zero = [Vec2::ZERO; SLOTS];
        let mut plan = Plan::new();
        let mut marker = 0;
        for (tick, y) in [(0u32, 6.0f32), (100, 6.0 + shift)] {
            att[target_att] = Vec2::new(28.0, y);
            let inp = Inputs {
                tick,
                tick_hz: c.physics.time.tick_hz,
                pitch_length: len,
                pitch_width: c.physics.pitch.width_m,
                ball: Vec2::new(70.0, 0.0),
                ball_vel: Vec2::ZERO,
                owner: 1,
                owner_facing: Vec2::ZERO,
                owner_vel: Vec2::ZERO,
                def_pos: &base,
                def_vel: &zero,
                att_pos: &att,
                anchors: &base,
                attackers: &attackers,
                lost_possession: false,
                engage: false,
            };
            state.step(&c.defense, &t, &inp, &mut plan);
            marker = (1..SLOTS)
                .find(|&i| plan.kind[i] == DUTY_MARK)
                .expect("опекун назначен");
        }
        marker
    };
    let first_flip = |communication: f32| {
        let start = marker_after_shift(communication, 0.0);
        (0..120)
            .map(|k| k as f32 * 0.25)
            .find(|&sh| marker_after_shift(communication, sh) != start)
    };
    let (low, high) = (first_flip(0.0), first_flip(1.0));
    println!("сдвиг подопечного до смены опекуна: коммуникация 0 → {low:?} м, коммуникация 1 → {high:?} м");
    let high = high.expect("при сильной коммуникации опека передаётся");
    // Слабая коммуникация: опекун меняется позже или не меняется вовсе в пределах проверенного сдвига.
    assert!(
        low.is_none_or(|l| l > high),
        "при слабой коммуникации опека передаётся позже ({low:?} м против {high} м)"
    );
}
