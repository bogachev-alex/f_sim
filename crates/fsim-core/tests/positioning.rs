//! Форма команды: якоря, линия обороны, компактность, сдвиг к мячу, роли, сценарии.

use fsim_core::config::{Config, Scenario, Style};
use fsim_core::embedded;
use fsim_core::metrics::{shape_metrics, ReversalCounter, ShapeMetrics};
use fsim_core::positioning::anchors::{compute, Pitch, ShapeInput};
use fsim_core::positioning::team::{Group, TeamSetup};
use fsim_core::positioning::{team_to_world, world_to_team};
use fsim_core::scenario_sim::ScenarioSim;
use fsim_core::sink::NoopSink;
use glam::Vec2;

const SLOTS: usize = 11;

fn cfg() -> Config {
    embedded::config().unwrap()
}

fn style(cfg: &Config, name: &str) -> Style {
    cfg.styles[name].clone()
}

fn setup(cfg: &Config, formation: &str, st: &Style) -> TeamSetup {
    TeamSetup::uniform(cfg, formation, st, cfg.sandbox.uniform_attr).unwrap()
}

fn pitch(cfg: &Config) -> Pitch {
    Pitch {
        length: cfg.physics.pitch.length_m,
        width: cfg.physics.pitch.width_m,
    }
}

fn anchors(cfg: &Config, team: &TeamSetup, ball: Vec2, w: f32) -> [Vec2; SLOTS] {
    let mut out = [Vec2::ZERO; SLOTS];
    compute(
        team,
        &cfg.positioning,
        pitch(cfg),
        ShapeInput { ball, attack_w: w },
        &[0.0; SLOTS],
        &mut out,
    );
    out
}

fn mean_x(team: &TeamSetup, a: &[Vec2; SLOTS], group: Option<Group>) -> f32 {
    let xs: Vec<f32> = (0..SLOTS)
        .filter(|&i| {
            team.slots[i].group != Group::Keeper && group.is_none_or(|g| team.slots[i].group == g)
        })
        .map(|i| a[i].x)
        .collect();
    xs.iter().sum::<f32>() / xs.len() as f32
}

fn extent(team: &TeamSetup, a: &[Vec2; SLOTS], f: impl Fn(Vec2) -> f32) -> f32 {
    let v: Vec<f32> = (0..SLOTS)
        .filter(|&i| team.slots[i].group != Group::Keeper)
        .map(|i| f(a[i]))
        .collect();
    v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min)
}

fn centre_ball(cfg: &Config) -> Vec2 {
    Vec2::new(cfg.physics.pitch.length_m * 0.5, 0.0)
}

#[test]
fn line_height_raises_back_line_in_both_phases() {
    let c = cfg();
    for w in [0.0, 1.0] {
        let mut prev = f32::MIN;
        for lh in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut st = style(&c, "balanced");
            st.line_height = lh;
            let t = setup(&c, "4-3-3", &st);
            let x = mean_x(&t, &anchors(&c, &t, centre_ball(&c), w), Some(Group::Back));
            assert!(
                x > prev,
                "w={w}: линия не растёт с line_height ({lh}): {x} <= {prev}"
            );
            prev = x;
        }
    }
}

#[test]
fn block_height_raises_front_line() {
    let c = cfg();
    let mut prev = f32::MIN;
    for bh in [0.0, 0.5, 1.0] {
        let mut st = style(&c, "balanced");
        st.block_height = bh;
        st.compactness = 0.0;
        st.line_height = 0.0;
        let t = setup(&c, "4-3-3", &st);
        let a = anchors(&c, &t, centre_ball(&c), 0.0);
        let front = (0..SLOTS).map(|i| a[i].x).fold(f32::MIN, f32::max);
        assert!(front > prev);
        prev = front;
    }
}

#[test]
fn compactness_shrinks_depth_and_width_in_defense() {
    let c = cfg();
    let (mut pd, mut pw) = (f32::MAX, f32::MAX);
    for k in [0.0, 0.5, 1.0] {
        let mut st = style(&c, "balanced");
        st.compactness = k;
        st.block_height = 1.0;
        st.line_height = 0.0;
        let t = setup(&c, "4-3-3", &st);
        let a = anchors(&c, &t, centre_ball(&c), 0.0);
        let (d, w) = (extent(&t, &a, |p| p.x), extent(&t, &a, |p| p.y));
        assert!(d < pd, "глубина не убывает с compactness={k}: {d} >= {pd}");
        assert!(w < pw, "ширина не убывает с compactness={k}: {w} >= {pw}");
        pd = d;
        pw = w;
    }
}

#[test]
fn width_in_widens_attack() {
    let c = cfg();
    let mut prev = 0.0;
    for wi in [0.0, 0.5, 1.0] {
        let mut st = style(&c, "balanced");
        st.width_in = wi;
        let t = setup(&c, "4-3-3", &st);
        let w = extent(&t, &anchors(&c, &t, centre_ball(&c), 1.0), |p| p.y);
        assert!(w > prev);
        prev = w;
    }
}

#[test]
fn block_shifts_toward_ball_laterally_more_when_compact() {
    let c = cfg();
    let shift = |compactness: f32, w: f32| {
        let mut st = style(&c, "balanced");
        st.compactness = compactness;
        let t = setup(&c, "4-3-3", &st);
        let mid = c.physics.pitch.length_m * 0.5;
        let left = anchors(&c, &t, Vec2::new(mid, -25.0), w);
        let right = anchors(&c, &t, Vec2::new(mid, 25.0), w);
        let m = |a: &[Vec2; SLOTS]| (0..SLOTS).map(|i| a[i].y).sum::<f32>() / SLOTS as f32;
        (m(&right) - m(&left)) / 2.0
    };
    assert!(
        shift(0.5, 0.0) > 5.0,
        "блок в обороне заметно уходит к мячу"
    );
    assert!(
        shift(1.0, 0.0) > shift(0.0, 0.0),
        "компактный блок сдвигается сильнее"
    );
    assert!(
        shift(0.5, 1.0) < shift(0.5, 0.0),
        "в атаке сдвиг меньше, чем в обороне"
    );
}

#[test]
fn line_follows_ball_along_the_pitch() {
    let c = cfg();
    let t = setup(&c, "4-3-3", &style(&c, "balanced"));
    let l = c.physics.pitch.length_m;
    let at = |bx: f32, w: f32| {
        mean_x(
            &t,
            &anchors(&c, &t, Vec2::new(bx, 0.0), w),
            Some(Group::Back),
        )
    };
    for w in [0.0, 1.0] {
        assert!(
            at(0.9 * l, w) > at(0.1 * l, w),
            "линия идёт за мячом (w={w})"
        );
    }
}

#[test]
fn anchors_stay_on_the_pitch_and_keep_formation() {
    let c = cfg();
    let p = pitch(&c);
    for formation in c.formations.0.keys() {
        for sname in c.styles.keys() {
            let t = setup(&c, formation, &c.styles[sname]);
            for w in [0.0, 0.5, 1.0] {
                for ball in [
                    Vec2::new(5.0, -30.0),
                    Vec2::new(52.0, 0.0),
                    Vec2::new(100.0, 30.0),
                    Vec2::new(10.0, 33.0),
                ] {
                    let a = anchors(&c, &t, ball, w);
                    for (i, v) in a.iter().enumerate() {
                        assert!(
                            v.x >= 0.0 && v.x <= p.length && v.y.abs() <= p.width * 0.5,
                            "{formation}/{sname}: игрок {i} вне поля: {v}"
                        );
                    }
                    // Игроки не наслаиваются: между любыми двумя не меньше 2 м.
                    let mut min_d = f32::MAX;
                    for i in 0..SLOTS {
                        for j in i + 1..SLOTS {
                            min_d = min_d.min(a[i].distance(a[j]));
                        }
                    }
                    assert!(
                        min_d > 2.0,
                        "{formation}/{sname} w={w} мяч={ball}: два якоря на расстоянии {min_d} м"
                    );
                    // Порядок слева направо внутри ряда схемы сохраняется.
                    for i in 0..SLOTS {
                        for j in 0..SLOTS {
                            let (si, sj) = (&t.slots[i], &t.slots[j]);
                            let same_row = si.group == sj.group
                                && (si.x - sj.x).abs() < 0.03 * p.length
                                && si.group != Group::Keeper;
                            if same_row
                                && si.y + 1.0 < sj.y
                                && si.role.contains("inverted") == sj.role.contains("inverted")
                            {
                                // Инвертированные роли могут пересекаться с соседями по ряду: их пропускаем.
                                assert!(
                                    a[i].y < a[j].y + 1e-3,
                                    "{formation}/{sname} w={w}: порядок нарушен ({} и {})",
                                    si.role,
                                    sj.role
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn mirrored_teams_get_mirrored_anchors() {
    let c = cfg();
    let t = setup(&c, "4-3-3", &style(&c, "gegenpress"));
    let half = c.physics.pitch.length_m * 0.5;
    let ball_world = Vec2::new(20.0, -12.0);
    for w in [0.0, 1.0] {
        let a0 = anchors(&c, &t, world_to_team(0, ball_world, half), w);
        // Команда 1 с зеркальным мячом даёт зеркальные в мировой системе точки.
        let a1 = anchors(&c, &t, world_to_team(1, -ball_world, half), w);
        for i in 0..SLOTS {
            let (p0, p1) = (team_to_world(0, a0[i], half), team_to_world(1, a1[i], half));
            assert!(
                (p0 + p1).length() < 1e-3,
                "игрок {i}: {p0} и {p1} не зеркальны"
            );
        }
    }
}

#[test]
fn roles_change_attacking_positions() {
    let c = cfg();
    let t = setup(&c, "4-3-3", &style(&c, "balanced"));
    let a = anchors(&c, &t, centre_ball(&c), 1.0);
    let idx = |role: &str| (0..SLOTS).find(|&i| t.slots[i].role == role).unwrap();
    let (inv, classic) = (idx("inverted_winger"), idx("classic_winger"));
    assert!(
        a[inv].y.abs() < a[classic].y.abs() - 8.0,
        "инвертированный вингер уходит в центр, классический остаётся у бровки"
    );
    // Накладывающийся защитник выше инвертированного.
    let mut st = style(&c, "balanced");
    st.fullback_role = fsim_core::config::FullbackRole::Inverted;
    let ti = setup(&c, "4-3-3", &st);
    let ai = anchors(&c, &ti, centre_ball(&c), 1.0);
    let fb = (0..SLOTS)
        .find(|&i| t.slots[i].pos == fsim_core::data::Position::FB)
        .unwrap();
    assert!(a[fb].x > ai[fb].x + 8.0 && ai[fb].y.abs() < a[fb].y.abs() - 8.0);
}

// ---------- сценарии ----------

fn scenario(c: &Config, name: &str) -> Scenario {
    c.scenarios[name].clone()
}

fn sim(c: &Config, sc: &Scenario, styles: [&str; 2], forms: [&str; 2]) -> ScenarioSim<NoopSink> {
    let teams = [0, 1].map(|k| setup(c, forms[k], &c.styles[styles[k]]));
    let mut s = ScenarioSim::new(1, c, sc, teams, NoopSink);
    // Тесты формы изолируют зональные якоря: оборона без мяча проверяется в `defense.rs`.
    s.defense_enabled = false;
    s
}

/// Полная симуляция сценария, с обороной без мяча.
fn sim_full(c: &Config, sc: &Scenario) -> ScenarioSim<NoopSink> {
    let teams = [0, 1].map(|k| setup(c, &sc.formations[k], &c.styles[&sc.styles[k]]));
    ScenarioSim::new(1, c, sc, teams, NoopSink)
}

fn run_seconds(s: &mut ScenarioSim<NoopSink>, c: &Config, secs: f32) {
    s.run((secs * c.physics.time.tick_hz as f32) as u32);
}

/// Сценарий с постоянным владением команды 0 и неподвижным мячом: команда 1 обороняется.
fn static_ball(c: &Config, ball: Vec2, secs: f32) -> Scenario {
    let mut sc = scenario(c, "shape_hold");
    sc.possession.truncate(1);
    sc.duration_s = secs;
    sc.ball = vec![
        fsim_core::config::BallPoint {
            t: 0.0,
            x: ball.x,
            y: ball.y,
            z: 0.0,
        },
        fsim_core::config::BallPoint {
            t: secs,
            x: ball.x,
            y: ball.y,
            z: 0.0,
        },
    ];
    sc
}

fn defending_shape(c: &Config, style_name: &str) -> ShapeMetrics {
    let sc = static_ball(c, Vec2::ZERO, 40.0);
    let mut s = sim(c, &sc, ["balanced", style_name], ["4-3-3", "4-3-3"]);
    run_seconds(&mut s, c, 40.0);
    shape_metrics(&s.world, s.team(1), 1, c.physics.pitch.length_m * 0.5)
}

#[test]
fn defending_line_orders_by_line_height_across_presets() {
    let c = cfg();
    let mut rows: Vec<(f32, f32, String)> = c
        .styles
        .iter()
        .map(|(n, s)| (s.line_height, defending_shape(&c, n).back_x, n.clone()))
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for w in rows.windows(2) {
        if w[1].0 - w[0].0 >= 0.1 {
            assert!(
                w[1].1 > w[0].1 + 1.0,
                "линия {} (lh {}) = {:.1} м не выше линии {} (lh {}) = {:.1} м",
                w[1].2,
                w[1].0,
                w[1].1,
                w[0].2,
                w[0].0,
                w[0].1
            );
        }
    }
    // Низкий блок стоит у своей штрафной, высокий пресс высоко.
    let low = rows.iter().find(|r| r.2 == "low_block_counter").unwrap().1;
    let high = rows.iter().find(|r| r.2 == "gegenpress").unwrap().1;
    assert!(low < 22.0, "низкий блок: линия {low:.1} м");
    assert!(
        high > low + 15.0,
        "высокая линия {high:.1} м против низкой {low:.1} м"
    );
}

#[test]
fn two_presets_differ_visibly_in_shape() {
    let c = cfg();
    let pos = defending_shape(&c, "positional");
    let low = defending_shape(&c, "low_block_counter");
    assert!(
        pos.back_x - low.back_x > 15.0,
        "линии: {:.1} против {:.1}",
        pos.back_x,
        low.back_x
    );
    assert!(
        pos.mean_x - low.mean_x > 15.0,
        "середины блока: {:.1} против {:.1}",
        pos.mean_x,
        low.mean_x
    );
    assert!(
        pos.depth > low.depth + 3.0,
        "глубины: {:.1} против {:.1}",
        pos.depth,
        low.depth
    );
}

#[test]
fn shape_holds_without_ball_movement() {
    let c = cfg();
    let sc = scenario(&c, "shape_hold");
    let mut s = sim(
        &c,
        &sc,
        ["positional", "low_block_counter"],
        ["4-3-3", "4-3-3"],
    );
    let hz = c.physics.time.tick_hz as f32;
    let mut worst: f32 = 0.0;
    for _ in 0..(15.0 * hz) as u32 {
        s.step();
        let p = &s.world.players;
        for i in 0..22 {
            worst = worst
                .max(Vec2::new(p.pos_x[i] - p.anchor_x[i], p.pos_y[i] - p.anchor_y[i]).length());
        }
    }
    // Рассеяние линии (OU) делает якоря слегка подвижными; ошибка слежения мала.
    assert!(
        worst < 3.5,
        "игрок отстал от якоря на {worst:.1} м при неподвижном мяче"
    );
}

#[test]
fn tracking_error_is_small_while_ball_moves() {
    let c = cfg();
    for name in ["ball_sweep", "switch_play", "possession_flips"] {
        let sc = scenario(&c, name);
        let mut s = sim(
            &c,
            &sc,
            [&sc.styles[0], &sc.styles[1]],
            [&sc.formations[0], &sc.formations[1]],
        );
        let hz = c.physics.time.tick_hz as f32;
        let (mut sum, mut n) = (0.0f64, 0u64);
        for _ in 0..(sc.duration_s * hz) as u32 {
            s.step();
            let p = &s.world.players;
            for i in 0..22 {
                sum += Vec2::new(p.pos_x[i] - p.anchor_x[i], p.pos_y[i] - p.anchor_y[i]).length()
                    as f64;
                n += 1;
            }
        }
        let mean = sum / n as f64;
        assert!(mean < 3.0, "{name}: средняя ошибка слежения {mean:.2} м");
    }
}

#[test]
fn physics_limits_hold_in_scenarios() {
    let c = cfg();
    for name in c.scenarios.keys() {
        let sc = scenario(&c, name);
        let mut s = sim_full(&c, &sc);
        let hz = c.physics.time.tick_hz as f32;
        let dt = c.physics.dt();
        let lim = (c
            .physics
            .player
            .acceleration_max_ms2
            .max(c.physics.player.deceleration_ms2)
            .powi(2)
            + c.physics.player.lateral_accel_max_ms2.powi(2))
        .sqrt();
        let mut prev = s.world.players.clone();
        for _ in 0..(sc.duration_s * hz) as u32 {
            s.step();
            let p = &s.world.players;
            for i in 0..22 {
                let sp = Vec2::new(p.vel_x[i], p.vel_y[i]).length();
                assert!(
                    sp <= s.world.body[i].max_speed + 1e-3,
                    "{name}: скорость {sp}"
                );
                let dv = Vec2::new(p.vel_x[i] - prev.vel_x[i], p.vel_y[i] - prev.vel_y[i]).length();
                assert!(dv <= lim * dt + 1e-2, "{name}: рывок скорости {dv}");
                assert!(
                    p.pos_x[i].abs() <= 52.5 + 1.0 && p.pos_y[i].abs() <= 34.0 + 1.0,
                    "{name}: игрок вне поля"
                );
            }
            prev = p.clone();
        }
    }
}

/// Порог детектора дрожания: смен направления на игрока в минуту.
const MAX_REVERSALS_PER_PLAYER_MINUTE: f32 = 1.5;

/// Детектор дрожания: частота смен направления движения (`docs/07-validation.md` §9).
#[test]
fn no_jitter_in_any_scenario() {
    let c = cfg();
    for name in c.scenarios.keys() {
        let sc = scenario(&c, name);
        let mut s = sim_full(&c, &sc);
        let hz = c.physics.time.tick_hz;
        let mut rc = ReversalCounter::default();
        // Отсчёты раз в 0,5 с: порог угла 120 градусов, движение быстрее 1 м/с.
        for tick in 0..(sc.duration_s * hz as f32) as u32 {
            s.step();
            if tick % (hz / 2) == 0 {
                rc.sample(&s.world, 1.0, 120f32.to_radians());
            }
        }
        // Частота смен направления на игрока в минуту; порог предварительный, уточняется в M7.
        let player_minutes = 22.0 * sc.duration_s / 60.0;
        let rate = rc.reversals as f32 / player_minutes;
        println!(
            "{name}: смен направления {} за {player_minutes:.1} игрока-минут ({rate:.2} в минуту)",
            rc.reversals
        );
        assert!(
            rate < MAX_REVERSALS_PER_PLAYER_MINUTE,
            "{name}: дрожание, {rate:.2} смен направления в минуту"
        );
    }
}

#[test]
fn scenarios_are_deterministic() {
    let c = cfg();
    for name in c.scenarios.keys() {
        let sc = scenario(&c, name);
        let run = || {
            let mut s = sim_full(&c, &sc);
            s.run(400);
            s.state_hash()
        };
        assert_eq!(run(), run(), "{name}");
    }
}
