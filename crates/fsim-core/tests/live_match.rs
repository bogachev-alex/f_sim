//! Матч: физика, владение, передачи, приём, отбор, удары, детекторы патологий, детерминизм.

use fsim_core::config::Config;
use fsim_core::embedded;
use fsim_core::live_match::{BallState, LiveMatch, MatchStats};
use fsim_core::metrics::ReversalCounter;
use fsim_core::positioning::team::TeamSetup;
use fsim_core::rng::match_seed;
use fsim_core::sink::NoopSink;
use fsim_core::world::PLAYERS;
use glam::Vec2;

fn cfg() -> Config {
    embedded::config().unwrap()
}

fn team(c: &Config, style: &str, formation: &str, quality: f32) -> TeamSetup {
    TeamSetup::uniform(c, formation, &c.styles[style], quality).unwrap()
}

fn new_match(c: &Config, seed: u64, a: &str, b: &str) -> LiveMatch<NoopSink> {
    let q = c.sandbox.uniform_attr;
    LiveMatch::new(
        seed,
        c,
        [team(c, a, "4-3-3", q), team(c, b, "4-3-3", q)],
        NoopSink,
    )
}

fn hz(c: &Config) -> u32 {
    c.physics.time.tick_hz
}

const STYLE_PAIRS: [(&str, &str); 3] = [
    ("positional", "gegenpress"),
    ("low_block_counter", "direct_long_ball"),
    ("wing_play", "balanced"),
];

#[test]
fn match_runs_with_valid_state_throughout() {
    let c = cfg();
    let (hl, hw) = (
        c.physics.pitch.length_m * 0.5,
        c.physics.pitch.width_m * 0.5,
    );
    for (seed, (a, b)) in [(1u64, STYLE_PAIRS[0]), (2, STYLE_PAIRS[1])] {
        let mut m = new_match(&c, seed, a, b);
        for tick in 0..(30 * 60 * hz(&c)) {
            m.step();
            if tick % 10 != 0 {
                continue;
            }
            let p = &m.world.players;
            for i in 0..PLAYERS {
                assert!(
                    p.pos_x[i].is_finite() && p.pos_y[i].is_finite(),
                    "позиция игрока {i}"
                );
                // Игрок может проскочить за линию при розыгрыше аута (боковое ускорение конечно).
                assert!(
                    p.pos_x[i].abs() <= hl + 3.0 && p.pos_y[i].abs() <= hw + 3.0,
                    "игрок {i} вне поля"
                );
                let sp = Vec2::new(p.vel_x[i], p.vel_y[i]).length();
                assert!(
                    sp <= m.world.body[i].max_speed + 1e-3,
                    "скорость игрока {i}: {sp}"
                );
            }
            let (bp, bv) = (m.world.ball_pos, m.world.ball_vel);
            assert!(bp.is_finite() && bv.is_finite(), "мяч");
            assert!(bp.z >= 0.0 && bp.z < 40.0, "высота мяча {}", bp.z);
            assert!(
                bv.length() <= c.physics.ball.shot_speed_max_ms + 6.0,
                "скорость мяча {}",
                bv.length()
            );
        }
        let s: MatchStats = m.stats;
        assert!(
            s.passes[0] > 100 && s.passes[1] > 100,
            "передач мало: {:?}",
            s.passes
        );
        assert!(s.possessions[0] > 20 && s.possessions[1] > 20);
    }
}

#[test]
fn player_motion_obeys_acceleration_limits() {
    let c = cfg();
    let mut m = new_match(&c, 3, "gegenpress", "positional");
    let dt = c.physics.dt();
    let lim = (c
        .physics
        .player
        .acceleration_max_ms2
        .max(c.physics.player.deceleration_ms2)
        .powi(2)
        + c.physics.player.lateral_accel_max_ms2.powi(2))
    .sqrt();
    let mut prev = m.world.players.clone();
    // Розыгрыш с центра телепортирует игроков (пауза после гола), поэтому проверяем между голами.
    let mut goals = 0;
    for _ in 0..(10 * 60 * hz(&c)) {
        m.step();
        let g = m.stats.goals[0] + m.stats.goals[1];
        let p = &m.world.players;
        if g == goals {
            for i in 0..PLAYERS {
                let dv = Vec2::new(p.vel_x[i] - prev.vel_x[i], p.vel_y[i] - prev.vel_y[i]).length();
                assert!(dv <= lim * dt + 1e-2, "игрок {i}: рывок скорости {dv}");
                let step =
                    Vec2::new(p.pos_x[i] - prev.pos_x[i], p.pos_y[i] - prev.pos_y[i]).length();
                assert!(
                    step <= m.world.body[i].max_speed * dt + 1e-3,
                    "игрок {i}: телепорт на {step} м"
                );
            }
        }
        goals = g;
        prev = p.clone();
    }
}

#[test]
fn carried_ball_stays_at_the_carriers_feet() {
    let c = cfg();
    let mut m = new_match(&c, 4, "positional", "gegenpress");
    let mut checked = 0;
    for _ in 0..(10 * 60 * hz(&c)) {
        m.step();
        if let BallState::Carried { team, slot } = m.state {
            let n = team as usize * 11 + slot as usize;
            let p = &m.world.players;
            let d = Vec2::new(
                p.pos_x[n] - m.world.ball_pos.x,
                p.pos_y[n] - m.world.ball_pos.y,
            )
            .length();
            assert!(
                d <= c.decision.control.carry_offset_m + 0.6,
                "мяч в {d} м от владельца"
            );
            checked += 1;
        }
    }
    assert!(checked > 2000);
}

#[test]
fn passes_leave_with_physical_speeds() {
    let c = cfg();
    let mut m = new_match(&c, 6, "positional", "gegenpress");
    let mut flights = 0;
    let mut was_flight = false;
    for _ in 0..(10 * 60 * hz(&c)) {
        m.step();
        let fl = matches!(m.state, BallState::Flight { .. });
        if fl && !was_flight {
            flights += 1;
            let sp = m.world.ball_vel.length();
            assert!(
                sp <= c.physics.ball.shot_speed_max_ms * 1.1,
                "скорость запуска {sp}"
            );
            assert!(sp >= 3.0, "слишком слабый запуск {sp}");
        }
        was_flight = fl;
    }
    assert!(flights > 100);
}

/// Детекторы патологий: пинг-понг и залипание у границы (`docs/07-validation.md` §9).
/// Пороги предварительные, уточняются в M7.
const MAX_PINGPONG: u32 = 6;
const MAX_STUCK_S: f32 = 25.0;
const MAX_DEAD_S: f32 = 20.0;

#[test]
fn no_pingpong_and_no_stuck_ball() {
    let c = cfg();
    for (seed, (a, b)) in STYLE_PAIRS.iter().enumerate() {
        let mut m = new_match(&c, 10 + seed as u64, a, b);
        let (mut dead_ticks, mut max_dead) = (0u32, 0u32);
        for _ in 0..(45 * 60 * hz(&c)) {
            m.step();
            if matches!(m.state, BallState::Dead { .. }) {
                dead_ticks += 1;
                max_dead = max_dead.max(dead_ticks);
            } else {
                dead_ticks = 0;
            }
        }
        let s = m.stats;
        println!(
            "{a} против {b}: пинг-понг {}, залипание {:.1} с, пауза мяча {} с",
            s.max_pingpong,
            s.max_stuck_s,
            max_dead / hz(&c)
        );
        assert!(
            s.max_pingpong <= MAX_PINGPONG,
            "{a}/{b}: цепочка {} передач между одной парой",
            s.max_pingpong
        );
        assert!(
            s.max_stuck_s <= MAX_STUCK_S,
            "{a}/{b}: мяч у границы {:.1} с подряд",
            s.max_stuck_s
        );
        assert!(
            max_dead as f32 / hz(&c) as f32 <= MAX_DEAD_S,
            "{a}/{b}: мяч вне игры {} с",
            max_dead / hz(&c)
        );
    }
}

/// Частота смен направления на игрока в минуту (детектор дрожания). Порог предварительный.
const MAX_REVERSALS_PER_PLAYER_MINUTE: f32 = 6.0;

#[test]
fn no_jitter_in_a_match() {
    let c = cfg();
    let mut m = new_match(&c, 7, "gegenpress", "positional");
    let mut rc = ReversalCounter::default();
    let minutes = 20.0;
    for tick in 0..(minutes * 60.0 * hz(&c) as f32) as u32 {
        m.step();
        if tick % (hz(&c) / 2) == 0 {
            rc.sample(&m.world, 1.0, 120f32.to_radians());
        }
    }
    let rate = rc.reversals as f32 / (22.0 * minutes);
    println!("смен направления в минуту на игрока: {rate:.2}");
    assert!(
        rate < MAX_REVERSALS_PER_PLAYER_MINUTE,
        "дрожание: {rate:.2} смен в минуту"
    );
}

#[test]
fn restarts_resume_play() {
    let c = cfg();
    let mut m = new_match(&c, 8, "direct_long_ball", "low_block_counter");
    m.run(30 * 60 * hz(&c));
    assert!(m.stats.out_of_play > 0, "мяч за линии не выходил ни разу");
    // После выходов игра продолжается: владений много.
    assert!(m.stats.possessions[0] + m.stats.possessions[1] > 50);
}

#[test]
fn stronger_players_complete_more_passes_and_win_more_duels() {
    let c = cfg();
    let (mut strong_acc, mut weak_acc) = (0.0f32, 0.0f32);
    let (mut strong_tk, mut weak_tk) = (0u32, 0u32);
    let (mut strong_dt, mut weak_dt) = (0u32, 0u32);
    for seed in 0..3 {
        let teams = [
            team(&c, "balanced", "4-3-3", 0.85),
            team(&c, "balanced", "4-3-3", 0.40),
        ];
        let mut m = LiveMatch::new(seed, &c, teams, NoopSink);
        m.run(30 * 60 * hz(&c));
        let s = m.stats;
        strong_acc += s.passes_ok[0] as f32 / s.passes[0].max(1) as f32;
        weak_acc += s.passes_ok[1] as f32 / s.passes[1].max(1) as f32;
        strong_tk += s.tackles_won[0];
        weak_tk += s.tackles_won[1];
        strong_dt += s.tackles_won[0] + s.tackles_failed[0];
        weak_dt += s.tackles_won[1] + s.tackles_failed[1];
    }
    assert!(
        strong_acc > weak_acc,
        "точность передач сильных {strong_acc} против слабых {weak_acc}"
    );
    let (sr, wr) = (
        strong_tk as f32 / strong_dt.max(1) as f32,
        weak_tk as f32 / weak_dt.max(1) as f32,
    );
    assert!(
        sr > wr,
        "доля выигранных отборов сильных {sr:.2} против слабых {wr:.2}"
    );
}

#[test]
fn shots_goals_and_xg_are_consistent() {
    let c = cfg();
    let (mut shots, mut on_target, mut goals, mut saves, mut xg) = (0u32, 0u32, 0u32, 0u32, 0.0f32);
    for seed in 0..4 {
        let mut m = new_match(&c, 20 + seed, "balanced", "balanced");
        m.run(45 * 60 * hz(&c));
        let s = m.stats;
        shots += s.shots[0] + s.shots[1];
        on_target += s.shots_on_target[0] + s.shots_on_target[1];
        goals += s.goals[0] + s.goals[1];
        saves += s.saves[0] + s.saves[1];
        xg += s.xg[0] + s.xg[1];
        for k in 0..2 {
            assert!(
                s.goals[k] <= s.shots_on_target[k],
                "голов больше, чем ударов в створ"
            );
            assert!(s.shots_on_target[k] <= s.shots[k]);
        }
    }
    println!("ударов {shots}, в створ {on_target}, голов {goals}, сейвов {saves}, xG {xg:.1}");
    assert!(shots > 20 && goals > 0, "ударов {shots}, голов {goals}");
    assert!(saves > 0, "вратари не сейвят");
    // xG остаётся контролем: на большой выборке сумма в разумном отношении к голам (калибровка в M8).
    let ratio = goals as f32 / xg.max(1e-3);
    assert!(
        (0.4..=3.0).contains(&ratio),
        "голов {goals} при xG {xg:.1} (отношение {ratio:.2})"
    );
}

#[test]
fn decisions_are_diverse_and_candidates_bounded() {
    let c = cfg();
    let mut m = new_match(&c, 9, "balanced", "balanced");
    let mut seen_rows = 0;
    for tick in 0..(5 * 60 * hz(&c)) {
        m.step();
        if tick % 40 == 0 {
            if let Some(rows) = m.explain_decision() {
                seen_rows += 1;
                assert!(rows.len() <= c.decision.candidates.stage1_keep as usize);
                for (cand, _) in &rows {
                    assert!(cand.u.is_finite() && cand.p.is_finite() && cand.v.is_finite());
                    assert!((0.0..=1.0).contains(&cand.p), "вероятность {}", cand.p);
                }
            }
            if let Some(d) = &m.last_decision {
                assert!(d.n_generated <= fsim_core::decision::MAX_CANDIDATES);
                assert!(d.n_top <= 8);
                assert!(d.temperature > 0.0);
            }
        }
    }
    assert!(seen_rows > 20);
    let d = m.stats.decisions;
    // Есть и пасы, и ведение, и удары.
    assert!(d[0] + d[1] + d[2] + d[3] > 20 && d[4] > 20, "решения {d:?}");
}

#[test]
fn temperature_is_higher_for_weaker_decision_makers() {
    use fsim_core::attrs::Attr;
    let c = cfg();
    let temp = |decisions: f32| {
        let mut t = team(&c, "balanced", "4-3-3", c.sandbox.uniform_attr);
        t.mental[6].decisions = {
            let curves = fsim_core::curves::CurveSet::build(&c.curves).unwrap();
            curves.eval(Attr::decisions, decisions)
        };
        let other = team(&c, "balanced", "4-3-3", c.sandbox.uniform_attr);
        let mut m = LiveMatch::new(1, &c, [t, other], NoopSink);
        // Розыгрыш начинает игрок слота 6 команды 0 (ближайший к центру).
        for _ in 0..(3 * hz(&c)) {
            m.step();
            if let Some(d) = &m.last_decision {
                return d.temperature;
            }
        }
        panic!("решения не было");
    };
    assert!(
        temp(0.2) > temp(0.9),
        "слабое мышление должно давать более высокую температуру"
    );
}

#[test]
fn match_is_deterministic() {
    let c = cfg();
    let a = fsim_core::live_match::run_match(
        5,
        6000,
        &c,
        ["positional", "gegenpress"],
        ["4-3-3", "4-3-3"],
    )
    .unwrap();
    let b = fsim_core::live_match::run_match(
        5,
        6000,
        &c,
        ["positional", "gegenpress"],
        ["4-3-3", "4-3-3"],
    )
    .unwrap();
    assert_eq!(a, b);
    let other = fsim_core::live_match::run_match(
        6,
        6000,
        &c,
        ["positional", "gegenpress"],
        ["4-3-3", "4-3-3"],
    )
    .unwrap();
    assert_ne!(a, other);
}

#[test]
fn batch_order_does_not_change_match_hashes() {
    let c = cfg();
    let run = |i: u64| {
        fsim_core::live_match::run_match(
            match_seed(11, i),
            2000,
            &c,
            ["wing_play", "balanced"],
            ["4-2-3-1", "4-4-2"],
        )
        .unwrap()
    };
    let fwd: Vec<u64> = (0..6).map(run).collect();
    let mut rev: Vec<u64> = (0..6).rev().map(run).collect();
    rev.reverse();
    assert_eq!(fwd, rev);
}
