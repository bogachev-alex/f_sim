//! Поведение двух команд без мяча и вокруг мяча: фазы атаки и обороны, якоря формы, линия
//! обороны, обязанности защитников, движение к целям. Общая часть сценариев (`scenario_sim`)
//! и матча (`live_match`): они отличаются тем, откуда берутся мяч и владелец.

use crate::config::{Config, DefenseConfig, PositioningConfig};
use crate::defense::{select_owner, DefenseState, Inputs, Plan, DUTY_OWNER, DUTY_ZONAL, NO_REF};
use crate::physics::body::step_body;
use crate::positioning::anchors::{compute, Pitch, ShapeInput};
use crate::positioning::line::LineState;
use crate::positioning::steering::speed_cap;
use crate::positioning::team::TeamSetup;
use crate::positioning::{lerp, team_to_world, vec_to_team, world_to_team};
use crate::rng::Rng;
use crate::world::World;
use glam::Vec2;

pub const SLOTS: usize = 11;

/// Как часто (в тиках) игрок пересчитывает направление взгляда.
const FACING_PERIOD_TICKS: u32 = 3;

/// Поправки движения на тик: масштаб желаемой скорости (ведение с мячом, падение после
/// неудачного отбора) и срочность (рывок на максимуме без плавного подхода).
#[derive(Clone, Debug)]
pub struct MoveMods {
    pub cap_scale: [f32; crate::world::PLAYERS],
    pub urgent: [bool; crate::world::PLAYERS],
}

impl Default for MoveMods {
    fn default() -> Self {
        MoveMods {
            cap_scale: [1.0; crate::world::PLAYERS],
            urgent: [false; crate::world::PLAYERS],
        }
    }
}

pub struct TeamPlay {
    pub teams: [TeamSetup; 2],
    lines: [LineState; 2],
    pos_cfg: PositioningConfig,
    def_cfg: DefenseConfig,
    defense: [DefenseState; 2],
    plans: [Plan; 2],
    /// Владелец мяча в каждой команде (слот), пока она владеет.
    owner: [Option<usize>; 2],
    prev_owner_team: usize,
    pub pitch: Pitch,
    dt: f32,
    walk: f32,
    tick_hz: u32,
    /// Время смены фазы, с: для каждой команды по темпу стиля.
    transition_s: [f32; 2],
    /// Зональные якоря обеих команд в системе команды.
    pub anchors: [[Vec2; SLOTS]; 2],
    /// Нападающие не уходят за линию офсайда соперника (в матче; в сценариях выключено).
    pub offside_clamp: bool,
    have_offside: bool,
}

impl TeamPlay {
    pub fn new(cfg: &Config, teams: [TeamSetup; 2]) -> TeamPlay {
        let pos_cfg = cfg.positioning.clone();
        let transition_s = [0, 1].map(|k| {
            lerp(
                pos_cfg.phase.transition_slow_s,
                pos_cfg.phase.transition_fast_s,
                teams[k].style.tempo,
            )
        });
        let lines = [
            LineState::new(&teams[0], &pos_cfg.line),
            LineState::new(&teams[1], &pos_cfg.line),
        ];
        TeamPlay {
            teams,
            lines,
            pos_cfg,
            def_cfg: cfg.defense.clone(),
            defense: [DefenseState::new(), DefenseState::new()],
            plans: [Plan::new(), Plan::new()],
            owner: [None, None],
            prev_owner_team: 0,
            pitch: Pitch {
                length: cfg.physics.pitch.length_m,
                width: cfg.physics.pitch.width_m,
            },
            dt: cfg.physics.dt(),
            walk: cfg.physics.player.gait_walk_fraction,
            tick_hz: cfg.physics.time.tick_hz,
            transition_s,
            anchors: [[Vec2::ZERO; SLOTS]; 2],
            offside_clamp: false,
            have_offside: false,
        }
    }

    /// Все 22 тела в порядке игроков мира.
    pub fn bodies(&self) -> [crate::physics::body::BodyParams; crate::world::PLAYERS] {
        let mut body = [self.teams[0].body[0]; crate::world::PLAYERS];
        for (k, t) in self.teams.iter().enumerate() {
            body[k * SLOTS..(k + 1) * SLOTS].copy_from_slice(&t.body);
        }
        body
    }

    /// Задаёт владельца для расчёта события «потеря мяча» при старте.
    pub fn set_initial_owner_team(&mut self, team: usize) {
        self.prev_owner_team = team;
    }

    /// Вес фазы атаки каждой команды приближается к цели (1 у владеющей), линия обороны
    /// рассеивается. `instant` ставит веса сразу, без перехода.
    #[inline(never)]
    pub fn update_phase(
        &mut self,
        world: &mut World,
        owner_team: usize,
        rng: &mut Rng,
        instant: bool,
    ) {
        for k in 0..2 {
            let target = if k == owner_team { 1.0 } else { 0.0 };
            let rate = self.dt / self.transition_s[k];
            let w = &mut world.teams[k];
            if instant {
                w.attack_w = target;
            } else {
                w.attack_w += (target - w.attack_w).clamp(-rate, rate);
            }
            w.has_ball = k == owner_team;
            if !instant {
                self.lines[k].step(&self.pos_cfg.line, rng, self.dt);
            }
        }
    }

    /// Зональные якоря по схеме, фазе, стилю и мячу.
    #[inline(never)]
    pub fn update_anchors(&mut self, world: &mut World) {
        let half_len = self.pitch.length * 0.5;
        let ball = Vec2::new(world.ball_pos.x, world.ball_pos.y);
        for k in 0..2 {
            let input = ShapeInput {
                ball: world_to_team(k, ball, half_len),
                attack_w: world.teams[k].attack_w,
            };
            let line = compute(
                &self.teams[k],
                &self.pos_cfg,
                self.pitch,
                input,
                &self.lines[k].dev,
                &mut self.anchors[k],
            );
            if self.offside_clamp && self.have_offside {
                let opp_line =
                    world_to_team(k, Vec2::new(world.teams[1 - k].offside_x, 0.0), half_len).x;
                let limit = opp_line + self.pos_cfg.attack.offside_margin_m;
                for a in self.anchors[k].iter_mut().skip(1) {
                    a.x = a.x.min(limit);
                }
            }
            let w = input.attack_w;
            let line_x = lerp(line.defense, line.attack, w);
            world.teams[k].line_x = team_to_world(k, Vec2::new(line_x, 0.0), half_len).x;
            for i in 0..SLOTS {
                let wp = team_to_world(k, self.anchors[k][i], half_len);
                world.players.anchor_x[k * SLOTS + i] = wp.x;
                world.players.anchor_y[k * SLOTS + i] = wp.y;
            }
        }
    }

    /// Предпоследний игрок команды по глубине: линия офсайда.
    pub fn update_team_state(&mut self, world: &mut World) {
        self.have_offside = true;
        let half_len = self.pitch.length * 0.5;
        for k in 0..2 {
            let (mut lo, mut second) = (f32::MAX, f32::MAX);
            for i in 0..SLOTS {
                let n = k * SLOTS + i;
                let x = world_to_team(
                    k,
                    Vec2::new(world.players.pos_x[n], world.players.pos_y[n]),
                    half_len,
                )
                .x;
                if x < lo {
                    second = lo;
                    lo = x;
                } else if x < second {
                    second = x;
                }
            }
            world.teams[k].offside_x = team_to_world(k, Vec2::new(second, 0.0), half_len).x;
        }
    }

    /// Оборона и владелец мяча: обязанности защищающейся команды заменяют её зональные якоря,
    /// владелец атакующей команды идёт к мячу. `carrier` задаёт владельца явно (в матче), иначе
    /// владелец выбирается как ближайший к мячу (в сценариях). Без аллокаций.
    #[inline(never)]
    pub fn apply_defense(&mut self, world: &mut World, owner_team: usize, carrier: Option<usize>) {
        let half = self.pitch.length * 0.5;
        let d = 1 - owner_team;
        let lost = self.prev_owner_team == d && owner_team != d;
        self.prev_owner_team = owner_team;
        let ball = Vec2::new(world.ball_pos.x, world.ball_pos.y);
        let ball_vel = Vec2::new(world.ball_vel.x, world.ball_vel.y);
        let pl = &world.players;
        let mut att_world = [Vec2::ZERO; SLOTS];
        let mut att_pos = [Vec2::ZERO; SLOTS];
        let mut def_pos = [Vec2::ZERO; SLOTS];
        let mut def_vel = [Vec2::ZERO; SLOTS];
        for i in 0..SLOTS {
            let (a, b) = (owner_team * SLOTS + i, d * SLOTS + i);
            att_world[i] = Vec2::new(pl.pos_x[a], pl.pos_y[a]);
            att_pos[i] = world_to_team(d, att_world[i], half);
            def_pos[i] = world_to_team(d, Vec2::new(pl.pos_x[b], pl.pos_y[b]), half);
            def_vel[i] = vec_to_team(d, Vec2::new(pl.vel_x[b], pl.vel_y[b]));
        }
        let owner = match carrier {
            Some(c) => c,
            None => select_owner(
                self.owner[owner_team],
                &att_world,
                ball,
                self.def_cfg.owner.switch_margin_m,
            ),
        };
        self.owner[owner_team] = Some(owner);
        self.owner[d] = None;
        let facing = pl.facing[owner_team * SLOTS + owner];
        let inp = Inputs {
            tick: world.tick,
            tick_hz: self.tick_hz,
            pitch_length: self.pitch.length,
            pitch_width: self.pitch.width,
            ball: world_to_team(d, ball, half),
            ball_vel: vec_to_team(d, ball_vel),
            owner,
            owner_facing: vec_to_team(d, Vec2::new(libm::cosf(facing), libm::sinf(facing))),
            owner_vel: vec_to_team(
                d,
                Vec2::new(
                    pl.vel_x[owner_team * SLOTS + owner],
                    pl.vel_y[owner_team * SLOTS + owner],
                ),
            ),
            def_pos: &def_pos,
            def_vel: &def_vel,
            att_pos: &att_pos,
            anchors: &self.anchors[d],
            attackers: &self.teams[owner_team],
            lost_possession: lost,
            engage: self.offside_clamp,
        };
        self.defense[d].step(&self.def_cfg, &self.teams[d], &inp, &mut self.plans[d]);

        let plan = &self.plans[d];
        let p = &mut world.players;
        for i in 0..SLOTS {
            let n = d * SLOTS + i;
            let w = team_to_world(d, plan.target[i], half);
            p.anchor_x[n] = w.x;
            p.anchor_y[n] = w.y;
            p.duty[n] = plan.kind[i];
            p.duty_ref[n] = if plan.target_player[i] == NO_REF {
                NO_REF
            } else {
                (owner_team * SLOTS) as u8 + plan.target_player[i]
            };
        }
        for i in 0..SLOTS {
            let n = owner_team * SLOTS + i;
            p.duty[n] = if i == owner {
                DUTY_OWNER
            } else if i == 0 {
                0
            } else {
                DUTY_ZONAL
            };
            p.duty_ref[n] = NO_REF;
        }
        let on = owner_team * SLOTS + owner;
        p.anchor_x[on] = ball.x;
        p.anchor_y[on] = ball.y;
    }

    /// Мяч ничей или в полёте: обязанности сбрасываются на зональные, прессинга нет.
    pub fn clear_duties(&mut self, world: &mut World) {
        let p = &mut world.players;
        for n in 0..crate::world::PLAYERS {
            p.duty[n] = if n % SLOTS == 0 { 0 } else { DUTY_ZONAL };
            p.duty_ref[n] = NO_REF;
        }
        self.plans = [Plan::new(), Plan::new()];
    }

    /// Обязанности игроков по тику: сколько заняты активной обороной.
    pub fn pressers(&self, team: usize) -> usize {
        self.plans[team].pressers()
    }

    /// Занятые зональные места команды (для проверок назначений).
    pub fn zonal_slots(&self, team: usize) -> [Option<u8>; SLOTS] {
        self.defense[team].zonal_slots()
    }

    pub fn plan(&self, team: usize) -> &Plan {
        &self.plans[team]
    }

    /// Движение всех игроков к их целям (`anchor_*`) с ограничениями физики.
    #[inline(never)]
    pub fn move_players(&self, world: &mut World, look_at: Vec2, mods: &MoveMods) {
        let tick = world.tick;
        let p = &mut world.players;
        for k in 0..2 {
            for i in 0..SLOTS {
                let n = k * SLOTS + i;
                let target = Vec2::new(p.anchor_x[n], p.anchor_y[n]);
                let mut pos = Vec2::new(p.pos_x[n], p.pos_y[n]);
                let mut vel = Vec2::new(p.vel_x[n], p.vel_y[n]);
                let dist = (target - pos).length();
                let cap = if mods.urgent[n] {
                    world.body[n].max_speed
                } else {
                    speed_cap(
                        dist,
                        world.body[n].max_speed,
                        self.walk,
                        self.teams[k].work_rate[i],
                        &self.pos_cfg.steering,
                    )
                } * mods.cap_scale[n];
                p.target_x[n] = target.x;
                p.target_y[n] = target.y;
                p.speed_cap[n] = cap;
                step_body(
                    &mut pos,
                    &mut vel,
                    &world.body[n],
                    target,
                    cap,
                    true,
                    self.dt,
                );
                p.pos_x[n] = pos.x;
                p.pos_y[n] = pos.y;
                p.vel_x[n] = vel.x;
                p.vel_y[n] = vel.y;
                // Взгляд по ходу движения, на месте на мяч.
                let look = if vel.length_squared() > 0.09 {
                    vel
                } else {
                    look_at - pos
                };
                // Взгляд обновляется не каждый тик: арктангенс дорог, а поворот гладкий.
                if (tick + n as u32).is_multiple_of(FACING_PERIOD_TICKS) {
                    p.facing[n] = libm::atan2f(look.y, look.x);
                }
            }
        }
    }
}
