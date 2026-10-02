//! Симуляция сценария: мяч и владение заданы скриптом, игроки держат форму.
//! Каждый тик: якоря по схеме, фазе, стилю и мячу, затем движение к якорям с ограничениями
//! физики. Решений и правил нет: это стенд для формы команды (M2).

use crate::config::{Config, Scenario};
use crate::defense::Plan;
use crate::positioning::team::TeamSetup;
use crate::rng::{MatchRng, Stream};
use crate::sink::FrameSink;
use crate::team_play::{MoveMods, TeamPlay, SLOTS};
use crate::world::{World, PLAYERS};
use glam::{Vec2, Vec3};

/// Траектория мяча по точкам: кубическая интерполяция Эрмита со скоростями Катмулла—Рома.
pub struct BallScript {
    t: Vec<f32>,
    p: Vec<Vec3>,
    v: Vec<Vec3>,
}

impl BallScript {
    pub fn new(s: &Scenario, ball_radius: f32) -> BallScript {
        let t: Vec<f32> = s.ball.iter().map(|b| b.t).collect();
        let p: Vec<Vec3> = s
            .ball
            .iter()
            .map(|b| Vec3::new(b.x, b.y, b.z.max(ball_radius)))
            .collect();
        let n = p.len();
        // Скорость в узле: разность соседних точек (на концах односторонняя).
        let v: Vec<Vec3> = (0..n)
            .map(|k| {
                let (a, b) = (k.saturating_sub(1), (k + 1).min(n - 1));
                (p[b] - p[a]) / (t[b] - t[a])
            })
            .collect();
        BallScript { t, p, v }
    }

    /// Положение и скорость в момент `time` (после конца траектории мяч стоит).
    pub fn at(&self, time: f32) -> (Vec3, Vec3) {
        let n = self.t.len();
        if time >= self.t[n - 1] {
            return (self.p[n - 1], Vec3::ZERO);
        }
        let k = self
            .t
            .partition_point(|&x| x <= time)
            .saturating_sub(1)
            .min(n - 2);
        let h = self.t[k + 1] - self.t[k];
        let u = (time - self.t[k]) / h;
        let (u2, u3) = (u * u, u * u * u);
        let (h00, h10, h01, h11) = (
            2.0 * u3 - 3.0 * u2 + 1.0,
            u3 - 2.0 * u2 + u,
            -2.0 * u3 + 3.0 * u2,
            u3 - u2,
        );
        let pos = self.p[k] * h00
            + self.v[k] * (h * h10)
            + self.p[k + 1] * h01
            + self.v[k + 1] * (h * h11);
        let (d00, d10, d01, d11) = (
            6.0 * u2 - 6.0 * u,
            3.0 * u2 - 4.0 * u + 1.0,
            -6.0 * u2 + 6.0 * u,
            3.0 * u2 - 2.0 * u,
        );
        let vel = (self.p[k] * d00
            + self.v[k] * (h * d10)
            + self.p[k + 1] * d01
            + self.v[k + 1] * (h * d11))
            / h;
        (pos, vel)
    }
}

pub struct ScenarioSim<S: FrameSink> {
    pub world: World,
    /// Выключает оборону без мяча: игроки держат только зональные якоря (тесты формы M2).
    pub defense_enabled: bool,
    play: TeamPlay,
    script: BallScript,
    possession: Vec<(f32, u8)>,
    rng: MatchRng,
    sink: S,
    dt: f32,
    duration: f32,
}

impl<S: FrameSink> ScenarioSim<S> {
    pub fn new(
        seed: u64,
        cfg: &Config,
        scenario: &Scenario,
        teams: [TeamSetup; 2],
        sink: S,
    ) -> Self {
        let play = TeamPlay::new(cfg, teams);
        let script = BallScript::new(scenario, cfg.physics.ball.radius_m);
        let mut sim = ScenarioSim {
            world: World::new(play.bodies()),
            defense_enabled: true,
            play,
            script,
            possession: scenario.possession.iter().map(|p| (p.t, p.team)).collect(),
            rng: MatchRng::new(seed),
            sink,
            dt: cfg.physics.dt(),
            duration: scenario.duration_s,
        };
        sim.settle();
        sim
    }

    pub fn duration_s(&self) -> f32 {
        self.duration
    }

    fn has_ball_team(&self, t: f32) -> usize {
        let k = self
            .possession
            .partition_point(|&(pt, _)| pt <= t)
            .saturating_sub(1);
        self.possession[k].1 as usize
    }

    /// Начальная расстановка: игроки сразу в якорях на момент t = 0, без разбега.
    fn settle(&mut self) {
        let (bp, bv) = self.script.at(0.0);
        self.world.ball_pos = bp;
        self.world.ball_vel = bv;
        let owner = self.has_ball_team(0.0);
        self.play.set_initial_owner_team(owner);
        let rng = self.rng.get(Stream::Decision);
        self.play.update_phase(&mut self.world, owner, rng, true);
        self.play.update_anchors(&mut self.world);
        let p = &mut self.world.players;
        for k in 0..2 {
            for i in 0..SLOTS {
                let n = k * SLOTS + i;
                p.pos_x[n] = p.anchor_x[n];
                p.pos_y[n] = p.anchor_y[n];
                p.target_x[n] = p.anchor_x[n];
                p.target_y[n] = p.anchor_y[n];
                p.vel_x[n] = 0.0;
                p.vel_y[n] = 0.0;
                p.facing[n] = if k == 0 { 0.0 } else { core::f32::consts::PI };
            }
        }
        self.play.update_team_state(&mut self.world);
        let tick = self.world.tick;
        self.sink.frame(tick, &self.world);
    }

    /// Обязанности игроков по тику: сколько заняты активной обороной.
    pub fn pressers(&self, team: usize) -> usize {
        self.play.pressers(team)
    }

    /// Занятые зональные места команды (для проверок назначений).
    pub fn zonal_slots(&self, team: usize) -> [Option<u8>; SLOTS] {
        self.play.zonal_slots(team)
    }

    pub fn plan(&self, team: usize) -> &Plan {
        self.play.plan(team)
    }

    /// Один физический тик. Без аллокаций.
    pub fn step(&mut self) {
        let t = (self.world.tick + 1) as f32 * self.dt;
        let (bp, bv) = self.script.at(t);
        self.world.ball_pos = bp;
        self.world.ball_vel = bv;

        let owner = self.has_ball_team(t);
        let rng = self.rng.get(Stream::Decision);
        self.play.update_phase(&mut self.world, owner, rng, false);
        self.play.update_anchors(&mut self.world);
        if self.defense_enabled {
            self.play.apply_defense(&mut self.world, owner, None);
        }
        self.play
            .move_players(&mut self.world, Vec2::new(bp.x, bp.y), &MoveMods::default());
        self.play.update_team_state(&mut self.world);
        self.world.tick += 1;
        self.sink.frame(self.world.tick, &self.world);
    }

    pub fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step();
        }
    }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    pub fn team(&self, k: usize) -> &TeamSetup {
        &self.play.teams[k]
    }

    /// FNV-1a по битам состояния игроков, мяча и команд.
    pub fn state_hash(&self) -> u64 {
        let mut h: u64 = 0xCBF2_9CE4_8422_2325;
        let mut mix = |x: f32| {
            for b in x.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        let p = &self.world.players;
        for i in 0..PLAYERS {
            for v in [
                p.pos_x[i],
                p.pos_y[i],
                p.vel_x[i],
                p.vel_y[i],
                p.facing[i],
                p.anchor_x[i],
                p.anchor_y[i],
                f32::from(p.duty[i]),
            ] {
                mix(v);
            }
        }
        let w = &self.world;
        for v in [w.ball_pos, w.ball_vel] {
            mix(v.x);
            mix(v.y);
            mix(v.z);
        }
        for t in &w.teams {
            mix(t.attack_w);
            mix(t.line_x);
            mix(t.offside_x);
        }
        h
    }
}

/// Прогон сценария без визуализатора: команды из одинаковых игроков, стили и схемы из
/// сценария. Возвращает хэш итогового состояния. Нужен тестам детерминизма.
pub fn run_scenario(seed: u64, ticks: u32, cfg: &Config, name: &str) -> Result<u64, String> {
    let sc = cfg
        .scenarios
        .get(name)
        .ok_or_else(|| format!("сценарий `{name}` не найден"))?;
    let mut teams = Vec::with_capacity(2);
    for k in 0..2 {
        let style = cfg.styles.get(&sc.styles[k]).ok_or("стиль не найден")?;
        teams.push(TeamSetup::uniform(
            cfg,
            &sc.formations[k],
            style,
            cfg.sandbox.uniform_attr,
        )?);
    }
    let [a, b]: [TeamSetup; 2] = teams.try_into().map_err(|_| "две команды")?;
    let mut sim = ScenarioSim::new(seed, cfg, sc, [a, b], crate::sink::NoopSink);
    sim.run(ticks);
    Ok(sim.state_hash())
}
