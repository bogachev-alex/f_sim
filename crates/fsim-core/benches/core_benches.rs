//! Бенчмарки ядра. Имена фиксированы `docs/02-engine-core.md` §7: `arrival`, `pitch_control`,
//! `candidate_eval`, `interception`, `defense_assignment` и `full_match` (полный матч 90 минут).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fsim_core::arrival::arrival_all;
use fsim_core::defense::{DefenseState, Inputs, Plan};
use fsim_core::embedded;
use fsim_core::live_match::{run_match, LiveMatch};
use fsim_core::match_loop::MatchSim;
use fsim_core::pitch_control::ControlGrid;
use fsim_core::positioning::team::TeamSetup;
use fsim_core::sink::NoopSink;
use fsim_core::world::PLAYERS;
use glam::Vec2;

fn benches(c: &mut Criterion) {
    let cfg = embedded::config().unwrap();
    // Тиков в матче: два тайма по `half_minutes` при `tick_hz`.
    let ticks = 2 * cfg.physics.time.half_minutes * 60 * cfg.physics.time.tick_hz;

    // Времена прибытия 22 игроков к одной точке на разогнанном состоянии.
    let mut sim = MatchSim::sandbox(1, &cfg, NoopSink);
    sim.run(200);
    let react = [cfg.physics.time.reaction_delay_max_s; PLAYERS];
    let mut out = [0.0f32; PLAYERS];
    c.bench_function("arrival", |b| {
        let w = &sim.world;
        b.iter(|| {
            arrival_all(
                &w.players,
                &w.body,
                &react,
                black_box(Vec2::new(10.0, 5.0)),
                &mut out,
            );
            black_box(out[0])
        })
    });

    // Назначение обязанностей обороны: 10 игроков × (особые обязанности + 10 зональных мест).
    {
        let style = &cfg.styles["gegenpress"];
        let team = TeamSetup::uniform(&cfg, "4-3-3", style, cfg.sandbox.uniform_attr).unwrap();
        let slots = &cfg.formations.0["4-3-3"];
        let (l, w) = (cfg.physics.pitch.length_m, cfg.physics.pitch.width_m);
        let base: [Vec2; 11] =
            std::array::from_fn(|i| Vec2::new(slots[i].x * l, (slots[i].y - 0.5) * w));
        let att: [Vec2; 11] = std::array::from_fn(|i| Vec2::new(l - base[i].x, -base[i].y));
        let zero = [Vec2::ZERO; 11];
        let mut state = DefenseState::new();
        let mut plan = Plan::new();
        let mut tick = 0u32;
        c.bench_function("defense_assignment", |b| {
            b.iter(|| {
                tick += 100; // каждый вызов пересчитывает назначения
                let inp = Inputs {
                    tick,
                    tick_hz: cfg.physics.time.tick_hz,
                    pitch_length: l,
                    pitch_width: w,
                    ball: Vec2::new(60.0, 5.0),
                    ball_vel: Vec2::ZERO,
                    owner: 5,
                    owner_facing: Vec2::ZERO,
                    owner_vel: Vec2::ZERO,
                    def_pos: &base,
                    def_vel: &zero,
                    att_pos: &att,
                    anchors: &base,
                    attackers: &team,
                    lost_possession: false,
                    engage: false,
                };
                state.step(&cfg.defense, &team, &inp, &mut plan);
                black_box(plan.kind[1])
            })
        });
    }

    // Сетка контроля пространства 21×14 на состоянии матча.
    {
        let mut live = {
            let teams = ["positional", "gegenpress"].map(|n| {
                TeamSetup::uniform(&cfg, "4-3-3", &cfg.styles[n], cfg.sandbox.uniform_attr).unwrap()
            });
            LiveMatch::new(1, &cfg, teams, NoopSink)
        };
        live.run(1200);
        let mut grid = ControlGrid::new(cfg.physics.pitch.length_m, cfg.physics.pitch.width_m);
        c.bench_function("pitch_control", |b| {
            b.iter(|| {
                grid.update(&live.world.players, &live.world.body, 0.2, 0.5);
                black_box(grid.cells[3][4])
            })
        });

        // Оценка кандидатов владельца мяча: этапы 0–2 (аллоцирует отладочный вектор).
        while !matches!(live.state, fsim_core::live_match::BallState::Carried { .. }) {
            live.step();
        }
        c.bench_function("candidate_eval", |b| {
            b.iter(|| black_box(live.explain_decision().map(|v| v.len())))
        });

        // Перехват: времена прибытия 11 соперников к 8 точкам линии паса.
        let w = &live.world;
        c.bench_function("interception", |b| {
            b.iter(|| {
                let mut worst = f32::MAX;
                for j in 11..PLAYERS {
                    for k in 1..=8 {
                        let p = Vec2::new(-30.0 + 4.0 * k as f32, 5.0);
                        let t = fsim_core::arrival::time_to_reach(
                            Vec2::new(w.players.pos_x[j], w.players.pos_y[j]),
                            Vec2::new(w.players.vel_x[j], w.players.vel_y[j]),
                            &w.body[j],
                            0.25,
                            black_box(p),
                        );
                        worst = worst.min(t);
                    }
                }
                black_box(worst)
            })
        });
    }

    // Полный матч 90 минут: два прогона по 108 000 тиков за выборку.
    let mut group = c.benchmark_group("full_match_group");
    group.sample_size(10);
    group.bench_function("full_match", |b| {
        b.iter(|| {
            black_box(
                run_match(
                    black_box(1),
                    ticks,
                    &cfg,
                    ["positional", "gegenpress"],
                    ["4-3-3", "4-3-3"],
                )
                .unwrap(),
            )
        })
    });
    group.finish();
}

criterion_group!(core, benches);
criterion_main!(core);
