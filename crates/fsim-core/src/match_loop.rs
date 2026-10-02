//! Цикл матча. На M1 это песочница: игроки бегут к случайным точкам с разной интенсивностью,
//! мяч периодически бьют в случайную сторону. Это настоящая физика (тело, 3D-мяч, ветер,
//! дождь), но без футбола: решений и правил нет. Песочница проверяет физику, определяет
//! детерминизм и кормит визуализатор.

use crate::attrs::{Attrs, ATTR_COUNT};
use crate::config::{Config, PhysicsConfig, SandboxConfig};
use crate::curves::CurveSet;
use crate::physics::ball::{BallEnv, BallModel};
use crate::physics::body::{step_body, BodyParams, Gait};
use crate::rng::{MatchRng, Stream};
use crate::sink::{FrameSink, NoopSink};
use crate::world::{World, PLAYERS};
use glam::{Vec2, Vec3};

pub struct MatchSim<S: FrameSink> {
    pub world: World,
    rng: MatchRng,
    sink: S,
    ball: BallModel,
    env: BallEnv,
    dt: f32,
    half_length: f32,
    half_width: f32,
    retarget_ticks: u32,
    kick_ticks: u32,
    lift_ratio: f32,
    walk: f32,
    jog: f32,
    pass_speed: (f32, f32),
}

impl<S: FrameSink> MatchSim<S> {
    pub fn new(
        seed: u64,
        physics: &PhysicsConfig,
        sandbox: &SandboxConfig,
        body: [BodyParams; PLAYERS],
        sink: S,
    ) -> Self {
        let mut rng = MatchRng::new(seed);
        let mut world = World::new(body);
        let (hl, hw) = (physics.pitch.length_m * 0.5, physics.pitch.width_m * 0.5);
        let init = rng.get(Stream::Init);
        for i in 0..PLAYERS {
            world.players.pos_x[i] = init.range_f32(-hl, hl);
            world.players.pos_y[i] = init.range_f32(-hw, hw);
            world.players.target_x[i] = world.players.pos_x[i];
            world.players.target_y[i] = world.players.pos_y[i];
        }
        world.ball_pos = Vec3::new(0.0, 0.0, physics.ball.radius_m);
        let hz = physics.time.tick_hz as f32;
        MatchSim {
            world,
            rng,
            sink,
            ball: BallModel::new(physics),
            env: BallEnv {
                wind: Vec2::new(sandbox.wind_x_ms, sandbox.wind_y_ms),
                rain: sandbox.rain,
                pitch_bounce: sandbox.pitch_bounce,
            },
            dt: physics.dt(),
            half_length: hl,
            half_width: hw,
            retarget_ticks: (sandbox.retarget_period_s * hz) as u32,
            kick_ticks: (sandbox.kick_period_s * hz) as u32,
            lift_ratio: sandbox.kick_lift_max_ratio,
            walk: physics.player.gait_walk_fraction,
            jog: physics.player.gait_jog_fraction,
            pass_speed: (
                physics.ball.ground_pass_speed_min_ms,
                physics.ball.shot_speed_max_ms,
            ),
        }
    }

    /// Песочница со всеми игроками одинакового уровня `sandbox.uniform_attr`.
    pub fn sandbox(seed: u64, cfg: &Config, sink: S) -> Self {
        let curves = CurveSet::build(&cfg.curves).expect("кривые отклика проверены при загрузке");
        let attrs: Attrs = [cfg.sandbox.uniform_attr; ATTR_COUNT];
        let body = [BodyParams::from_attrs(&attrs, &curves, &cfg.physics); PLAYERS];
        Self::new(seed, &cfg.physics, &cfg.sandbox, body, sink)
    }

    /// Расстановка игроков: позиции и цели совпадают, скорость нулевая.
    pub fn set_player_positions(&mut self, xy: &[Vec2; PLAYERS]) {
        let p = &mut self.world.players;
        for (i, v) in xy.iter().enumerate() {
            p.pos_x[i] = v.x;
            p.pos_y[i] = v.y;
            p.target_x[i] = v.x;
            p.target_y[i] = v.y;
            p.vel_x[i] = 0.0;
            p.vel_y[i] = 0.0;
        }
    }

    pub fn sink_mut(&mut self) -> &mut S {
        &mut self.sink
    }

    /// Один физический тик. Без аллокаций.
    pub fn step(&mut self) {
        let tick = self.world.tick;
        let p = &mut self.world.players;
        for i in 0..PLAYERS {
            // Игроки переназначают цель по очереди, а не все в один тик.
            if (tick + i as u32 * 3).is_multiple_of(self.retarget_ticks) {
                let d = self.rng.get(Stream::Decision);
                p.target_x[i] = d.range_f32(-self.half_length, self.half_length);
                p.target_y[i] = d.range_f32(-self.half_width, self.half_width);
                let gait = match (d.next_f32() * 3.0) as u32 {
                    0 => Gait::Walk,
                    1 => Gait::Jog,
                    _ => Gait::Sprint,
                };
                p.speed_cap[i] = self.world.body[i].max_speed
                    * match gait {
                        Gait::Walk => self.walk,
                        Gait::Jog => self.jog,
                        Gait::Sprint => 1.0,
                    };
            }
            let mut pos = Vec2::new(p.pos_x[i], p.pos_y[i]);
            let mut vel = Vec2::new(p.vel_x[i], p.vel_y[i]);
            step_body(
                &mut pos,
                &mut vel,
                &self.world.body[i],
                Vec2::new(p.target_x[i], p.target_y[i]),
                p.speed_cap[i],
                true,
                self.dt,
            );
            p.pos_x[i] = pos.x;
            p.pos_y[i] = pos.y;
            p.vel_x[i] = vel.x;
            p.vel_y[i] = vel.y;
            if vel.length_squared() > 0.09 {
                p.facing[i] = libm::atan2f(vel.y, vel.x);
            }
        }
        if tick.is_multiple_of(self.kick_ticks) {
            let e = self.rng.get(Stream::Execution);
            let speed = e.range_f32(self.pass_speed.0, self.pass_speed.1);
            let ang = e.range_f32(0.0, core::f32::consts::TAU);
            let lift = e.range_f32(0.0, self.lift_ratio);
            let horiz = Vec2::new(libm::cosf(ang), libm::sinf(ang)) * speed;
            self.world.ball_vel = Vec3::new(horiz.x, horiz.y, speed * lift);
            self.world.ball_pos = Vec3::new(
                e.range_f32(-self.half_length * 0.5, self.half_length * 0.5),
                e.range_f32(-self.half_width * 0.5, self.half_width * 0.5),
                self.ball.radius(),
            );
        }
        let w = &mut self.world;
        self.ball
            .step(&mut w.ball_pos, &mut w.ball_vel, &self.env, self.dt);
        w.tick += 1;
        self.sink.frame(w.tick, w);
    }

    pub fn run(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.step();
        }
    }

    /// FNV-1a по битам состояния: одинаковый хэш означает побитово одинаковое состояние.
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
            mix(p.pos_x[i]);
            mix(p.pos_y[i]);
            mix(p.vel_x[i]);
            mix(p.vel_y[i]);
            mix(p.facing[i]);
        }
        let w = &self.world;
        for v in [w.ball_pos, w.ball_vel] {
            mix(v.x);
            mix(v.y);
            mix(v.z);
        }
        h
    }
}

/// Прогон песочницы без визуализатора. Возвращает хэш итогового состояния.
pub fn run_sandbox(seed: u64, ticks: u32, cfg: &Config) -> u64 {
    let mut sim = MatchSim::sandbox(seed, cfg, NoopSink);
    sim.run(ticks);
    sim.state_hash()
}
