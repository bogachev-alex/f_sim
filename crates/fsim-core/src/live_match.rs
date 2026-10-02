//! Матч: мяч по физике, владение, решения владельца, пасы с перехватом, приём, ведение,
//! отбор, потеря мяча и выход за линии (`docs/09-milestones.md`, M4). Без ударов и вратарских
//! сейвов (M4b), стандартов, судьи и усталости (M6).

use crate::arrival::time_to_reach;
use crate::config::{Config, DecisionConfig, PhysicsConfig};
use crate::decision::{self, Candidate, Commit, Ctx, Decision, Kind, NO_SLOT};
use crate::defense::{DUTY_CHASE, DUTY_RECEIVE};
use crate::physics::ball::{BallEnv, BallModel};
use crate::pitch_control::ControlGrid;
use crate::positioning::lerp;
use crate::positioning::team::TeamSetup;
use crate::positioning::{team_to_world, vec_to_team, world_to_team};
use crate::rng::{MatchRng, Stream};
use crate::sink::FrameSink;
use crate::team_play::{MoveMods, TeamPlay, SLOTS};
use crate::value::Xt;
use crate::world::{World, PLAYERS};
use glam::{Vec2, Vec3};

/// Как часто пересчитывается сетка контроля пространства, тиков (0,5 с при 20 Гц).
const CONTROL_PERIOD_TICKS: u32 = 10;
/// Реакция игроков в сетке контроля, с.
const CONTROL_REACT_S: f32 = 0.2;
/// Мягкость логистики контроля в сетке, с.
const CONTROL_SOFTNESS_S: f32 = 0.5;
/// Выше этой высоты контакт с мячом считается приёмом в воздухе, м.
const AIR_CONTACT_Z_M: f32 = 0.4;
/// Вратарь бросается за линию штанг не дальше этого запаса и встаёт чуть впереди линии ворот, м.
const SAVE_LATERAL_MARGIN_M: f32 = 0.8;
const SAVE_GOAL_LINE_OFFSET_M: f32 = 0.5;
/// Мяч ближе этого расстояния к боковой линии или линии ворот считается у границы, м.
const STUCK_EDGE_M: f32 = 3.0;
/// Игрок может зайти за боковую линию или линию ворот не дальше этого запаса, м.
const OUT_MARGIN_M: f32 = 0.5;
/// Точек прогноза траектории мяча для перехвата и погони.
const PREDICT_POINTS: usize = 16;
/// Длина прогноза, м.
const PREDICT_LENGTH_M: f32 = 45.0;
/// Мяч считается медленным и ничьим-стоящим ниже этой скорости, м/с.
const SLOW_BALL_MS: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BallState {
    /// Мяч у ног игрока.
    Carried { team: u8, slot: u8 },
    /// Мяч летит после паса: пасующий (общий индекс) и получатель, если он задан.
    Flight {
        team: u8,
        kicker: u8,
        receiver: Option<u8>,
    },
    /// Ничей мяч.
    Loose { last_team: u8 },
    /// Мяч вне игры, ждёт розыгрыша: команда, точка, исполнитель, оставшиеся тики.
    Dead { team: u8, taker: u8, ticks: u32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct MatchStats {
    pub passes: [u32; 2],
    pub passes_ok: [u32; 2],
    pub interceptions: [u32; 2],
    pub tackles_won: [u32; 2],
    pub tackles_failed: [u32; 2],
    /// Новые владения (получение мяча после ничьего, перехвата или отбора).
    pub possessions: [u32; 2],
    pub out_of_play: u32,
    pub goals: [u32; 2],
    /// Принятые решения по видам `decision::Kind` (по порядку объявления).
    pub decisions: [u32; 8],
    /// Передачи по видам: попыток, принятых партнёром, сумма оценённых вероятностей успеха.
    pub pass_attempts: [u32; 7],
    pub pass_completed: [u32; 7],
    pub pass_expected: [f32; 7],
    /// Причины неудачи паса по видам: перехват соперником, плохой приём партнёром, плохой
    /// приём соперником, мяч вне игры.
    pub pass_fail: [[u32; 4]; 7],
    /// Удары, удары в створ (гол или сейв), сейвы вратаря, блоки полевыми игроками и сумма xG.
    pub shots: [u32; 2],
    pub shots_on_target: [u32; 2],
    pub saves: [u32; 2],
    pub blocks: [u32; 2],
    pub xg: [f32; 2],
    /// Детекторы патологий (`docs/07-validation.md` §9): самая длинная цепочка передач между
    /// одной парой туда-обратно и самое долгое время мяча у границы поля, с.
    /// Калибровка оценок паса: по десятой доле ожидаемой вероятности попыток и принятых.
    pub pass_calibration: [[u32; 2]; 10],
    pub max_pingpong: u32,
    pub max_stuck_s: f32,
}

/// Удар в полёте: бьющая команда и оценённая вероятность гола.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotInfo {
    pub team: u8,
    pub xg: f32,
}

pub struct LiveMatch<S: FrameSink> {
    pub world: World,
    pub state: BallState,
    /// Удар в полёте, пока мяч не остановил вратарь, игрок или он не покинул поле.
    pub shot: Option<ShotInfo>,
    pub stats: MatchStats,
    /// Последнее решение владельца мяча (для отладки и тестов).
    pub last_decision: Option<Decision>,
    /// Включает журнал перехватов (аллоцирует; только для отладки): вид паса, расстояние от
    /// точки удара, время полёта и высота мяча в момент перехвата.
    pub trace_interceptions: bool,
    /// Журнал событий матча для отладки (аллоцирует): время, текст.
    pub trace_events: bool,
    pub event_log: Vec<(f32, String)>,
    /// Считать сетку контроля пространства (нужна визуализатору; решения считают контроль
    /// в нужных точках сами), раз в 0,5 с.
    pub control_grid_enabled: bool,
    pub interception_log: Vec<(u8, f32, f32, f32)>,
    kick_pos: Vec2,
    kick_tick: u32,
    play: TeamPlay,
    xt: Xt,
    ctrl: ControlGrid,
    ball_model: BallModel,
    env: BallEnv,
    phys: PhysicsConfig,
    dec: DecisionConfig,
    rng: MatchRng,
    sink: S,
    dt: f32,
    hz: u32,
    lockout: [u32; PLAYERS],
    stumble: [u32; PLAYERS],
    duel_cd: [u32; PLAYERS],
    next_decision: u32,
    /// Тик получения мяча текущим владельцем.
    carry_since: u32,
    /// Вид последнего паса (для учёта завершённых передач по видам).
    pass_kind: u8,
    /// Корзина калибровки текущего паса.
    pass_bucket: u8,
    /// Текущая цепочка передач одной пары: пара (кто, кому) и длина.
    pingpong: (u8, u8, u32),
    stuck_ticks: u32,
    commit: Commit,
    dribble_target: Vec2,
    last_touch_team: usize,
    prev_ball: Vec3,
    /// Точка, куда погашен последний вынос `Dead` (для розыгрыша).
    dead_point: Vec2,
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-x))
}

impl<S: FrameSink> LiveMatch<S> {
    pub fn new(seed: u64, cfg: &Config, teams: [TeamSetup; 2], sink: S) -> Self {
        let mut play = TeamPlay::new(cfg, teams);
        play.offside_clamp = true;
        let mut m = LiveMatch {
            world: World::new(play.bodies()),
            state: BallState::Dead {
                team: 0,
                taker: 0,
                ticks: 0,
            },
            shot: None,
            stats: MatchStats::default(),
            last_decision: None,
            trace_interceptions: false,
            trace_events: false,
            event_log: Vec::new(),
            control_grid_enabled: false,
            interception_log: Vec::new(),
            kick_pos: Vec2::ZERO,
            kick_tick: 0,
            play,
            xt: Xt::new(
                cfg.xt.clone(),
                cfg.physics.pitch.length_m,
                cfg.physics.pitch.width_m,
            ),
            ctrl: ControlGrid::new(cfg.physics.pitch.length_m, cfg.physics.pitch.width_m),
            ball_model: BallModel::new(&cfg.physics),
            env: BallEnv::default(),
            phys: cfg.physics.clone(),
            dec: cfg.decision.clone(),
            rng: MatchRng::new(seed),
            sink,
            dt: cfg.physics.dt(),
            hz: cfg.physics.time.tick_hz,
            lockout: [0; PLAYERS],
            stumble: [0; PLAYERS],
            duel_cd: [0; PLAYERS],
            next_decision: 0,
            carry_since: 0,
            pass_kind: 0,
            pass_bucket: 0,
            pingpong: (255, 255, 0),
            stuck_ticks: 0,
            commit: Commit::default(),
            dribble_target: Vec2::ZERO,
            last_touch_team: 0,
            prev_ball: Vec3::ZERO,
            dead_point: Vec2::ZERO,
        };
        m.kickoff(0, true);
        m.sync_ball_flags();
        let t = m.world.tick;
        m.sink.frame(t, &m.world);
        m
    }

    fn secs_to_ticks(&self, s: f32) -> u32 {
        (s * self.hz as f32).ceil() as u32
    }

    // ---------- розыгрыш с центра ----------

    /// Расстановка по схемам и мяч в центре; право розыгрыша у команды `team`.
    fn kickoff(&mut self, team: usize, first: bool) {
        self.shot = None;
        let rng = self.rng.get(Stream::Dynamics);
        self.play.update_phase(&mut self.world, team, rng, true);
        self.play.update_anchors(&mut self.world);
        let p = &mut self.world.players;
        for n in 0..PLAYERS {
            p.pos_x[n] = p.anchor_x[n];
            p.pos_y[n] = p.anchor_y[n];
            p.target_x[n] = p.anchor_x[n];
            p.target_y[n] = p.anchor_y[n];
            p.vel_x[n] = 0.0;
            p.vel_y[n] = 0.0;
            p.facing[n] = if n < SLOTS {
                0.0
            } else {
                core::f32::consts::PI
            };
        }
        self.lockout = [0; PLAYERS];
        self.stumble = [0; PLAYERS];
        self.duel_cd = [0; PLAYERS];
        self.world.ball_pos = Vec3::new(0.0, 0.0, self.phys.ball.radius_m);
        self.world.ball_vel = Vec3::ZERO;
        self.prev_ball = self.world.ball_pos;
        self.dead_point = Vec2::ZERO;
        self.last_touch_team = team;
        // Розыгрыш с центра: ближайший к мячу полевой игрок команды.
        let taker = self.nearest_outfield(team, Vec2::ZERO);
        let wait = if first {
            0.5
        } else {
            self.dec.restart.goal_dead_s
        };
        self.state = BallState::Dead {
            team: team as u8,
            taker: taker as u8,
            ticks: self.secs_to_ticks(wait),
        };
        self.play.update_team_state(&mut self.world);
    }

    fn nearest_outfield(&self, team: usize, p: Vec2) -> usize {
        let pl = &self.world.players;
        (1..SLOTS)
            .map(|i| team * SLOTS + i)
            .min_by(|&a, &b| {
                let (da, db) = (
                    Vec2::new(pl.pos_x[a], pl.pos_y[a]).distance_squared(p),
                    Vec2::new(pl.pos_x[b], pl.pos_y[b]).distance_squared(p),
                );
                da.partial_cmp(&db).unwrap()
            })
            .unwrap()
    }

    // ---------- вспомогательное ----------

    fn pos(&self, n: usize) -> Vec2 {
        Vec2::new(self.world.players.pos_x[n], self.world.players.pos_y[n])
    }

    fn vel(&self, n: usize) -> Vec2 {
        Vec2::new(self.world.players.vel_x[n], self.world.players.vel_y[n])
    }

    fn ball2(&self) -> Vec2 {
        Vec2::new(self.world.ball_pos.x, self.world.ball_pos.y)
    }

    /// Команда, которой принадлежит фаза атаки.
    fn possession_team(&self) -> usize {
        match self.state {
            BallState::Carried { team, .. }
            | BallState::Flight { team, .. }
            | BallState::Dead { team, .. } => team as usize,
            BallState::Loose { last_team } => last_team as usize,
        }
    }

    /// Давление соперников около игрока: сумма `1 − d/R`, нормированная.
    fn pressure_around(&self, team: usize, p: Vec2) -> f32 {
        let r = self.dec.pressure.radius_m;
        let sum: f32 = (0..SLOTS)
            .map(|i| (1.0 - self.pos((1 - team) * SLOTS + i).distance(p) / r).max(0.0))
            .sum();
        (sum / 1.5).min(1.0)
    }

    fn acquire(&mut self, n: usize) {
        let team = n / SLOTS;
        let slot = n % SLOTS;
        self.state = BallState::Carried {
            team: team as u8,
            slot: slot as u8,
        };
        self.shot = None;
        self.last_touch_team = team;
        let awareness = self.play.teams[team].skills[slot].awareness;
        let react = lerp(self.dec.reaction.max_s, self.dec.reaction.min_s, awareness);
        self.next_decision = self.world.tick + self.secs_to_ticks(react);
        self.commit = Commit::default();
        self.carry_since = self.world.tick;
        self.dribble_target = self.pos(n);
        self.stats.possessions[team] += 1;
    }

    // ---------- шаг ----------

    /// Один физический тик.
    pub fn step(&mut self) {
        self.prev_ball = self.world.ball_pos;
        match self.state {
            BallState::Carried { .. } | BallState::Dead { .. } => {}
            _ => {
                let w = &mut self.world;
                self.ball_model
                    .step(&mut w.ball_pos, &mut w.ball_vel, &self.env, self.dt);
            }
        }
        self.track_stuck();
        for v in self
            .lockout
            .iter_mut()
            .chain(self.stumble.iter_mut())
            .chain(self.duel_cd.iter_mut())
        {
            *v = v.saturating_sub(1);
        }

        if matches!(
            self.state,
            BallState::Flight { .. } | BallState::Loose { .. }
        ) {
            self.check_out_of_play();
        }
        if matches!(
            self.state,
            BallState::Flight { .. } | BallState::Loose { .. }
        ) {
            self.resolve_contact();
        }
        if matches!(self.state, BallState::Carried { .. }) {
            self.resolve_duels();
        }
        if matches!(self.state, BallState::Carried { .. }) {
            self.maybe_decide();
        }
        self.update_dead();

        let owner_team = self.possession_team();
        let rng = self.rng.get(Stream::Dynamics);
        self.play
            .update_phase(&mut self.world, owner_team, rng, false);
        self.play.update_anchors(&mut self.world);
        let mut mods = MoveMods::default();
        match self.state {
            BallState::Carried { team, slot } => {
                self.play
                    .apply_defense(&mut self.world, team as usize, Some(slot as usize));
                self.steer_carrier(team as usize, slot as usize, &mut mods);
            }
            _ => {
                self.play.clear_duties(&mut self.world);
                self.assign_chasers(&mut mods);
            }
        }
        for n in 0..PLAYERS {
            if self.stumble[n] > 0 {
                mods.cap_scale[n] *= 0.3;
                mods.urgent[n] = false;
            }
        }
        // Цели игроков не выходят за пределы поля дальше небольшого запаса (погоня за мячом
        // у линии).
        let (hl, hw) = (
            self.phys.pitch.length_m * 0.5,
            self.phys.pitch.width_m * 0.5,
        );
        let p = &mut self.world.players;
        for n in 0..PLAYERS {
            p.anchor_x[n] = p.anchor_x[n].clamp(-hl - OUT_MARGIN_M, hl + OUT_MARGIN_M);
            p.anchor_y[n] = p.anchor_y[n].clamp(-hw - OUT_MARGIN_M, hw + OUT_MARGIN_M);
        }
        let look = self.ball2();
        self.play.move_players(&mut self.world, look, &mods);
        self.play.update_team_state(&mut self.world);

        // Мяч у ног следует за владельцем.
        if let BallState::Carried { team, slot } = self.state {
            self.place_carried_ball(team as usize * SLOTS + slot as usize);
        }
        if self.control_grid_enabled && self.world.tick.is_multiple_of(CONTROL_PERIOD_TICKS) {
            self.ctrl.update(
                &self.world.players,
                &self.world.body,
                CONTROL_REACT_S,
                CONTROL_SOFTNESS_S,
            );
        }
        self.world.tick += 1;
        self.sync_ball_flags();
        self.sink.frame(self.world.tick, &self.world);
    }

    /// Состояние мяча для кадра визуализатора.
    fn sync_ball_flags(&mut self) {
        let (mode, actor) = match self.state {
            BallState::Carried { team, slot } => (1, team * SLOTS as u8 + slot),
            BallState::Flight { kicker, .. } => (2, kicker),
            BallState::Loose { .. } => (3, 255),
            BallState::Dead { taker, .. } => (4, taker),
        };
        self.world.ball_mode = mode;
        self.world.ball_actor = actor;
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

    pub fn control_grid(&self) -> &ControlGrid {
        &self.ctrl
    }

    pub fn pressers(&self, team: usize) -> usize {
        self.play.pressers(team)
    }

    /// Мяч у ног: перед игроком по ходу движения (или по взгляду, если стоит).
    fn place_carried_ball(&mut self, n: usize) {
        let (v, f) = (self.vel(n), self.world.players.facing[n]);
        let dir = if v.length_squared() > 0.25 {
            v.normalize()
        } else {
            Vec2::new(libm::cosf(f), libm::sinf(f))
        };
        let p = self.pos(n) + dir * self.dec.control.carry_offset_m;
        self.world.ball_pos = Vec3::new(p.x, p.y, self.phys.ball.radius_m);
        self.world.ball_vel = Vec3::new(v.x, v.y, 0.0);
    }

    // ---------- выход за линии и гол ----------

    #[inline(never)]
    fn check_out_of_play(&mut self) {
        let (hl, hw) = (
            self.phys.pitch.length_m * 0.5,
            self.phys.pitch.width_m * 0.5,
        );
        let b = self.world.ball_pos;
        let r = self.phys.ball.radius_m;
        let touch = self.last_touch_team;
        if b.y.abs() > hw + r {
            // Аут: розыгрыш у боковой линии за соперника последнего касавшегося.
            let point = Vec2::new(b.x.clamp(-hl + 1.0, hl - 1.0), b.y.signum() * (hw - 0.4));
            self.start_dead(1 - touch, point, false);
        } else if b.x.abs() > hl + r {
            let side = b.x.signum();
            let in_mouth =
                b.y.abs() < self.phys.goal.width_m * 0.5 && b.z < self.phys.goal.height_m;
            if in_mouth {
                // Гол в ворота `side`: забила команда, атакующая эти ворота.
                let scorer = if side > 0.0 { 0 } else { 1 };
                self.stats.goals[scorer] += 1;
                self.log(|| format!("ГОЛ команда {scorer}"));
                if self.shot.is_some_and(|sh| sh.team as usize == scorer) {
                    self.stats.shots_on_target[scorer] += 1;
                }
                self.shot = None;
                self.kickoff(1 - scorer, false);
                return;
            }
            // Мяч за линией ворот: атаковавшая команда коснулась последней — удар от ворот,
            // иначе угловой.
            let attacked_by = if side > 0.0 { 0 } else { 1 };
            if touch == attacked_by {
                self.start_dead(1 - attacked_by, Vec2::new(side * (hl - 5.5), 0.0), true);
            } else {
                self.start_dead(
                    1 - touch,
                    Vec2::new(side * (hl - 0.4), b.y.signum() * (hw - 0.4)),
                    false,
                );
            }
        }
    }

    fn start_dead(&mut self, team: usize, point: Vec2, goalkeeper_takes: bool) {
        self.stats.out_of_play += 1;
        if matches!(self.state, BallState::Flight { .. }) && self.shot.is_none() {
            self.stats.pass_fail[self.pass_kind as usize][3] += 1;
        }
        self.shot = None;
        self.world.ball_pos = Vec3::new(point.x, point.y, self.phys.ball.radius_m);
        self.world.ball_vel = Vec3::ZERO;
        self.dead_point = point;
        let taker = if goalkeeper_takes {
            team * SLOTS
        } else {
            self.nearest_outfield(team, point)
        };
        let ticks = self.secs_to_ticks(self.dec.restart.dead_s);
        self.state = BallState::Dead {
            team: team as u8,
            taker: taker as u8,
            ticks,
        };
        self.last_touch_team = team;
    }

    /// Исполнитель подбегает к мячу; по истечении паузы он берёт мяч.
    fn update_dead(&mut self) {
        let BallState::Dead { team, taker, ticks } = self.state else {
            return;
        };
        let ticks = ticks.saturating_sub(1);
        self.state = BallState::Dead { team, taker, ticks };
        let n = taker as usize;
        if ticks == 0 && self.pos(n).distance(self.dead_point) <= self.dec.restart.arrive_m {
            self.world.ball_vel = Vec3::ZERO;
            self.acquire(n);
        }
    }

    // ---------- контакт с мячом ----------

    #[inline(never)]
    fn resolve_contact(&mut self) {
        let c = self.dec.control.clone();
        let (a, b) = (self.prev_ball, self.world.ball_pos);
        let (a2, b2) = (Vec2::new(a.x, a.y), Vec2::new(b.x, b.y));
        let mut best: Option<(f32, usize)> = None;
        let save = self.dec.shot.save.clone();
        // Вратарь, защищающий ворота от удара, дотягивается дальше и выше полевого игрока.
        let saver = self.shot.map(|sh| (1 - sh.team as usize) * SLOTS);
        for n in 0..PLAYERS {
            if self.lockout[n] > 0 {
                continue;
            }
            let (reach, height) = if Some(n) == saver {
                (save.reach_m, save.reach_height_m)
            } else {
                (c.reach_m, c.reach_height_m)
            };
            let d = decision_dist_segment(self.pos(n), a2, b2);
            if d <= reach && b.z <= height && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, n));
            }
        }
        let Some((_, n)) = best else { return };
        let team = n / SLOTS;
        let slot = n % SLOTS;
        if self.shot.is_some() && Some(n) == saver {
            self.resolve_save(n);
            return;
        }
        let (kicker_team, kicker) = match self.state {
            BallState::Flight { team, kicker, .. } => (Some(team as usize), Some(kicker as usize)),
            _ => (None, None),
        };
        let interceptor = kicker_team.is_some_and(|k| k != team);
        let mut rel =
            (Vec2::new(self.world.ball_vel.x, self.world.ball_vel.y) - self.vel(n)).length();
        // Мяч в воздухе гасят грудью или головой: относительная скорость приёма меньше.
        if self.world.ball_pos.z > AIR_CONTACT_Z_M {
            rel *= decision::LOFT_RECEIVE_SPEED_FACTOR;
        }
        let press = self.pressure_around(team, self.pos(n));
        let skill = self.play.teams[team].skills[slot].control;
        let bonus = if interceptor { c.intercept_bonus } else { 0.0 };
        let p = sigmoid(
            c.base + c.skill_gain * skill - c.speed_cost * rel - c.pressure_cost * press + bonus,
        );
        let roll = self.rng.get(Stream::Duels).next_f32();
        if roll < p {
            if let Some(k) = kicker_team.filter(|_| self.shot.is_none()) {
                if Some(n) != kicker {
                    if k == team {
                        self.stats.passes_ok[k] += 1;
                        self.stats.pass_completed[self.pass_kind as usize] += 1;
                        self.stats.pass_calibration[self.pass_bucket as usize][1] += 1;
                        self.track_pingpong(kicker.unwrap_or(0), n);
                    } else {
                        self.stats.interceptions[team] += 1;
                        self.log(|| format!("перехват команда {team} игрок {slot}"));
                        self.stats.pass_fail[self.pass_kind as usize][0] += 1;
                        if self.trace_interceptions {
                            self.interception_log.push((
                                self.pass_kind,
                                self.ball2().distance(self.kick_pos),
                                (self.world.tick - self.kick_tick) as f32 / self.hz as f32,
                                self.world.ball_pos.z,
                            ));
                        }
                    }
                }
            }
            if self.shot.is_some_and(|sh| sh.team as usize != team) {
                self.stats.blocks[team] += 1;
            }
            self.acquire(n);
        } else {
            if self.shot.is_some_and(|sh| sh.team as usize != team) {
                self.stats.blocks[team] += 1;
            }
            if let Some(k) = kicker_team.filter(|_| self.shot.is_none()) {
                if Some(n) != kicker {
                    let reason = if k == team { 1 } else { 2 };
                    self.stats.pass_fail[self.pass_kind as usize][reason] += 1;
                }
            }
            // Неудачный приём: мяч отскакивает в случайную сторону.
            let (angle, mag) = {
                let r = self.rng.get(Stream::Duels);
                (
                    r.range_f32(0.0, core::f32::consts::TAU),
                    r.range_f32(0.5, 1.0),
                )
            };
            let v = Vec2::new(self.world.ball_vel.x, self.world.ball_vel.y) * 0.35
                + Vec2::new(libm::cosf(angle), libm::sinf(angle)) * c.deflect_speed_ms * mag;
            self.world.ball_vel = Vec3::new(v.x, v.y, 0.0);
            self.world.ball_pos.z = self.phys.ball.radius_m;
            self.lockout[n] = self.secs_to_ticks(c.lockout_s);
            self.last_touch_team = team;
            self.shot = None;
            self.state = BallState::Loose {
                last_team: team as u8,
            };
        }
    }

    // ---------- единоборства ----------

    #[inline(never)]
    fn resolve_duels(&mut self) {
        let BallState::Carried { team, slot } = self.state else {
            return;
        };
        let (team, c) = (team as usize, team as usize * SLOTS + slot as usize);
        let cp = self.pos(c);
        let duel = self.dec.duel.clone();
        let carrier_sk = self.play.teams[team].skills[slot as usize];
        for j in 0..SLOTS {
            let n = (1 - team) * SLOTS + j;
            if self.stumble[n] > 0 || self.duel_cd[n] > 0 || self.pos(n).distance(cp) > duel.range_m
            {
                continue;
            }
            let mental = self.play.teams[1 - team].mental[j];
            let rate = duel.attempt_rate_hz
                * self.dt
                * (1.0 + duel.aggression_gain * (mental.aggression - 0.5));
            if self.rng.get(Stream::Duels).next_f32() >= rate {
                continue;
            }
            let tackle = self.play.teams[1 - team].skills[j].tackle;
            let speed_diff = self.vel(n).length() - self.vel(c).length();
            let x = duel.skill_gain * (tackle - 0.5 * (carrier_sk.dribble + carrier_sk.shield))
                + duel.speed_gain * speed_diff;
            let won = self.rng.get(Stream::Duels).next_f32() < sigmoid(x);
            if won {
                self.stats.tackles_won[1 - team] += 1;
                let takes = self.rng.get(Stream::Duels).next_f32() < duel.win_possession_share;
                if takes {
                    self.acquire(n);
                } else {
                    let away = (cp - self.pos(n)).normalize_or_zero();
                    self.world.ball_vel =
                        Vec3::new(away.x * duel.push_ms, away.y * duel.push_ms, 0.0);
                    self.lockout[c] = self.secs_to_ticks(self.dec.control.lockout_s);
                    self.last_touch_team = 1 - team;
                    self.state = BallState::Loose {
                        last_team: (1 - team) as u8,
                    };
                }
            } else {
                self.stats.tackles_failed[1 - team] += 1;
                self.stumble[n] = self.secs_to_ticks(duel.stumble_s);
                self.duel_cd[n] = self.secs_to_ticks(duel.cooldown_s);
            }
            return; // не больше одной попытки за тик
        }
    }

    // ---------- решение владельца ----------

    /// Положения игроков в системе команды `k` для решений.
    /// Сколько секунд текущий владелец держит мяч.
    fn held_seconds(&self) -> f32 {
        (self.world.tick.saturating_sub(self.carry_since)) as f32 / self.hz as f32
    }

    fn snapshot(&self, k: usize) -> CtxSnapshot {
        let half = self.phys.pitch.length_m * 0.5;
        let mut s = CtxSnapshot {
            mates_pos: [Vec2::ZERO; SLOTS],
            mates_vel: [Vec2::ZERO; SLOTS],
            opp_pos: [Vec2::ZERO; SLOTS],
            opp_vel: [Vec2::ZERO; SLOTS],
        };
        for i in 0..SLOTS {
            s.mates_pos[i] = world_to_team(k, self.pos(k * SLOTS + i), half);
            s.mates_vel[i] = vec_to_team(k, self.vel(k * SLOTS + i));
            s.opp_pos[i] = world_to_team(k, self.pos((1 - k) * SLOTS + i), half);
            s.opp_vel[i] = vec_to_team(k, self.vel((1 - k) * SLOTS + i));
        }
        s
    }

    /// Разбор оценок текущего владельца: кандидаты после этапа 1 с составляющими вероятности
    /// паса. Для отладки и калибровки; аллоцирует.
    pub fn explain_decision(&self) -> Option<Vec<(Candidate, decision::PassTerms)>> {
        let BallState::Carried { team, slot } = self.state else {
            return None;
        };
        let (k, slot) = (team as usize, slot as usize);
        let snap = self.snapshot(k);
        let ctx = Ctx {
            cfg: &self.dec,
            phys: &self.phys,
            xt: &self.xt,
            me: slot,
            mates_pos: &snap.mates_pos,
            mates_vel: &snap.mates_vel,
            opp_pos: &snap.opp_pos,
            opp_vel: &snap.opp_vel,
            team: &self.play.teams[k],
            opp: &self.play.teams[1 - k],
            held_s: self.held_seconds(),
        };
        Some(decision::explain(&ctx))
    }

    #[inline(never)]
    fn maybe_decide(&mut self) {
        let BallState::Carried { team, slot } = self.state else {
            return;
        };
        if self.world.tick < self.next_decision {
            return;
        }
        let (k, slot) = (team as usize, slot as usize);
        let half = self.phys.pitch.length_m * 0.5;
        let mut mates_pos = [Vec2::ZERO; SLOTS];
        let mut mates_vel = [Vec2::ZERO; SLOTS];
        let mut opp_pos = [Vec2::ZERO; SLOTS];
        let mut opp_vel = [Vec2::ZERO; SLOTS];
        for i in 0..SLOTS {
            mates_pos[i] = world_to_team(k, self.pos(k * SLOTS + i), half);
            mates_vel[i] = vec_to_team(k, self.vel(k * SLOTS + i));
            opp_pos[i] = world_to_team(k, self.pos((1 - k) * SLOTS + i), half);
            opp_vel[i] = vec_to_team(k, self.vel((1 - k) * SLOTS + i));
        }
        let ctx = Ctx {
            cfg: &self.dec,
            phys: &self.phys,
            xt: &self.xt,
            me: slot,
            mates_pos: &mates_pos,
            mates_vel: &mates_vel,
            opp_pos: &opp_pos,
            opp_vel: &opp_vel,
            team: &self.play.teams[k],
            opp: &self.play.teams[1 - k],
            held_s: self.held_seconds(),
        };
        let rng = self.rng.get(Stream::Decision);
        let d = decision::decide(&ctx, rng, &mut self.commit, self.world.tick, self.hz);
        let chosen = d.chosen;
        let pressure = d.pressure;
        self.stats.decisions[chosen.kind as usize] += 1;
        self.last_decision = Some(d);
        self.next_decision = self.world.tick + self.secs_to_ticks(self.dec.period_s);
        match chosen.kind {
            Kind::Dribble => {
                self.dribble_target = team_to_world(k, chosen.point, half);
            }
            Kind::Shield => {
                self.dribble_target = self.pos(k * SLOTS + slot);
            }
            Kind::Shot => self.execute_shot(k, slot, &chosen, pressure),
            _ => self.execute_pass(
                k,
                slot,
                &chosen,
                pressure,
                &ctx_snapshot(&mates_pos, &mates_vel, &opp_pos, &opp_vel),
            ),
        }
    }

    /// Сейв: вратарь успевает к мячу удара; ловит или отбивает, иначе мяч летит дальше.
    fn resolve_save(&mut self, gk: usize) {
        let sv = self.dec.shot.save.clone();
        let team = gk / SLOTS;
        let skill = self.play.teams[team].skills[0].gk_shot_stop;
        let speed = self.world.ball_vel.length();
        let p = sigmoid(sv.base + sv.gain * skill - sv.speed_cost * speed);
        let (roll, catch_roll, spread) = {
            let r = self.rng.get(Stream::Duels);
            (r.next_f32(), r.next_f32(), r.range_f32(-1.0, 1.0))
        };
        self.lockout[gk] = self.secs_to_ticks(sv.lockout_s);
        if roll >= p {
            return; // вратарь не достал: мяч летит дальше к воротам
        }
        let shooter = self.shot.map_or(0, |sh| sh.team as usize);
        self.stats.saves[team] += 1;
        self.log(|| format!("СЕЙВ вратарь команды {team}"));
        self.stats.shots_on_target[shooter] += 1;
        if catch_roll < sv.catch_share {
            self.acquire(gk);
        } else {
            // Отбив в поле от ворот под случайным углом.
            let out = if team == 0 { 1.0 } else { -1.0 };
            let ang = spread * core::f32::consts::FRAC_PI_3;
            let v = Vec2::new(out * libm::cosf(ang), libm::sinf(ang)) * sv.parry_speed_ms;
            self.world.ball_vel = Vec3::new(v.x, v.y, 0.0);
            self.world.ball_pos.z = self.phys.ball.radius_m;
            self.last_touch_team = team;
            self.shot = None;
            self.state = BallState::Loose {
                last_team: team as u8,
            };
        }
    }

    /// Удар: прицел в дальний от вратаря угол или центр, ошибка по навыку и давлению, дальше
    /// физика, блок и вратарь.
    fn execute_shot(&mut self, k: usize, slot: usize, c: &Candidate, pressure: f32) {
        let sh = self.dec.shot.clone();
        let half = self.phys.pitch.length_m * 0.5;
        let n = k * SLOTS + slot;
        let from = self.pos(n);
        let goal_x = if k == 0 { half } else { -half };
        let gk = self.pos((1 - k) * SLOTS);
        let corner = self.phys.goal.width_m * 0.5 - sh.aim_inset_m;
        let far_side = if gk.y >= 0.0 { -1.0 } else { 1.0 };
        let sk = self.play.teams[k].skills[slot];
        let far_shot = from.distance(Vec2::new(goal_x, 0.0)) >= decision::LONG_SHOT_FROM_M;
        let skill = if far_shot { sk.long_shot } else { sk.finish };
        let composure = self.play.teams[k].mental[slot].composure;
        let (pick, ea, ev, es) = {
            let r = self.rng.get(Stream::Execution);
            (
                r.next_f32(),
                r.next_normal(),
                r.next_normal(),
                r.next_normal(),
            )
        };
        let y_aim = if pick < sh.far_corner_share {
            far_side * corner
        } else {
            0.0
        };
        let sigma = lerp(sh.angle_sigma_worst_deg, sh.angle_sigma_best_deg, skill)
            * (1.0 + sh.pressure_gain * pressure * (1.0 - composure));
        let sigma = sigma.to_radians();
        let to = Vec2::new(goal_x, y_aim);
        let dist = from.distance(to).max(1.0);
        let dir = (to - from) / dist;
        let (ca, sa) = (libm::cosf(ea * sigma), libm::sinf(ea * sigma));
        let dir = Vec2::new(dir.x * ca - dir.y * sa, dir.x * sa + dir.y * ca);
        let speed = (lerp(sh.speed_min_ms, sh.speed_max_ms, skill) * (1.0 + 0.03 * es))
            .clamp(sh.speed_min_ms * 0.7, self.phys.ball.shot_speed_max_ms);
        // Вертикальная составляющая: мяч приходит на высоту прицела с учётом падения и ошибки.
        let t = dist / speed;
        let z_target = sh.aim_height_m + dist * libm::tanf(ev * sigma);
        let vz = (z_target - self.phys.ball.radius_m) / t + 0.5 * self.phys.ball.gravity_ms2 * t;
        self.world.ball_vel = Vec3::new(dir.x * speed, dir.y * speed, vz);
        self.world.ball_pos.z = self.phys.ball.radius_m;
        self.state = BallState::Flight {
            team: k as u8,
            kicker: n as u8,
            receiver: None,
        };
        self.shot = Some(ShotInfo {
            team: k as u8,
            xg: c.p,
        });
        self.lockout[n] = self.secs_to_ticks(self.dec.control.lockout_s);
        self.stats.shots[k] += 1;
        self.stats.xg[k] += c.p;
        let dist_goal = from.distance(Vec2::new(goal_x, 0.0));
        self.log(|| {
            format!(
                "УДАР команда {} игрок {} xG {:.2} дистанция {:.1} м давление {:.2}",
                k, slot, c.p, dist_goal, pressure
            )
        });
        self.last_touch_team = k;
    }

    /// Время подряд, которое мяч проводит у границы поля (детектор залипания).
    #[inline(never)]
    fn track_stuck(&mut self) {
        let (hl, hw) = (
            self.phys.pitch.length_m * 0.5,
            self.phys.pitch.width_m * 0.5,
        );
        let b = self.world.ball_pos;
        let near_edge = b.y.abs() > hw - STUCK_EDGE_M || b.x.abs() > hl - STUCK_EDGE_M;
        if near_edge && !matches!(self.state, BallState::Dead { .. }) {
            self.stuck_ticks += 1;
            self.stats.max_stuck_s = self
                .stats
                .max_stuck_s
                .max(self.stuck_ticks as f32 / self.hz as f32);
        } else {
            self.stuck_ticks = 0;
        }
    }

    /// Цепочка передач туда-обратно между одной парой растёт, пока пара не меняется.
    fn track_pingpong(&mut self, from: usize, to: usize) {
        let (a, b, len) = self.pingpong;
        let pair_continues =
            (a as usize == to && b as usize == from) || (a as usize == from && b as usize == to);
        self.pingpong = if pair_continues {
            (from as u8, to as u8, len + 1)
        } else {
            (from as u8, to as u8, 1)
        };
        self.stats.max_pingpong = self.stats.max_pingpong.max(self.pingpong.2);
    }

    fn log(&mut self, text: impl FnOnce() -> String) {
        if self.trace_events {
            let t = self.world.tick as f32 / self.hz as f32;
            let msg = text();
            self.event_log.push((t, msg));
        }
    }

    /// Выполнение паса: идеальный запуск плюс ошибка по навыку и давлению, дальше физика.
    fn execute_pass(
        &mut self,
        k: usize,
        slot: usize,
        c: &Candidate,
        pressure: f32,
        snap: &CtxSnapshot,
    ) {
        let half = self.phys.pitch.length_m * 0.5;
        let n = k * SLOTS + slot;
        let from = self.pos(n);
        let to = team_to_world(k, c.point, half);
        let ctx = Ctx {
            cfg: &self.dec,
            phys: &self.phys,
            xt: &self.xt,
            me: slot,
            mates_pos: &snap.mates_pos,
            mates_vel: &snap.mates_vel,
            opp_pos: &snap.opp_pos,
            opp_vel: &snap.opp_vel,
            team: &self.play.teams[k],
            opp: &self.play.teams[1 - k],
            held_s: self.held_seconds(),
        };
        let launch = decision::pass_launch(&ctx, c.kind, snap.mates_pos[slot], c.point);
        let skill = match c.kind {
            Kind::PassFeet | Kind::PassSpace => self.play.teams[k].skills[slot].short_pass,
            Kind::Through => {
                0.5 * (self.play.teams[k].skills[slot].short_pass
                    + self.play.teams[k].skills[slot].long_pass)
            }
            _ => self.play.teams[k].skills[slot].long_pass,
        };
        let sig_a = decision_angle_sigma(&ctx, skill, pressure);
        let sig_v = decision_speed_sigma(&ctx, skill, pressure);
        let (ea, ev) = {
            let r = self.rng.get(Stream::Execution);
            (r.next_normal() * sig_a, r.next_normal() * sig_v)
        };
        let dir = (to - from).normalize_or_zero();
        let (ca, sa) = (libm::cosf(ea), libm::sinf(ea));
        let dir = Vec2::new(dir.x * ca - dir.y * sa, dir.x * sa + dir.y * ca);
        let speed = launch.speed_h * (1.0 + ev).max(0.3);
        self.world.ball_vel = Vec3::new(dir.x * speed, dir.y * speed, launch.vz);
        self.world.ball_pos.z = self.phys.ball.radius_m;
        let receiver = (c.slot != NO_SLOT).then_some((k * SLOTS + c.slot as usize) as u8);
        self.state = BallState::Flight {
            team: k as u8,
            kicker: n as u8,
            receiver,
        };
        self.lockout[n] = self.secs_to_ticks(self.dec.control.lockout_s);
        self.stats.passes[k] += 1;
        self.log(|| {
            format!(
                "пас {:?} команда {} слот {} -> {} p={:.2}",
                c.kind, k, slot, c.slot, c.p
            )
        });
        self.stats.pass_attempts[c.kind as usize] += 1;
        self.stats.pass_expected[c.kind as usize] += c.p;
        self.pass_kind = c.kind as u8;
        self.pass_bucket = ((c.p * 10.0) as usize).min(9) as u8;
        self.stats.pass_calibration[self.pass_bucket as usize][0] += 1;
        self.kick_pos = from;
        self.kick_tick = self.world.tick;
        self.last_touch_team = k;
    }

    // ---------- движение с мячом, погоня и перехват ----------

    fn steer_carrier(&mut self, team: usize, slot: usize, mods: &mut MoveMods) {
        let n = team * SLOTS + slot;
        let sk = self.play.teams[team].skills[slot];
        let slowdown = lerp(
            self.phys.player.ball_slowdown_max,
            self.phys.player.ball_slowdown_min,
            sk.dribble,
        );
        mods.cap_scale[n] = 1.0 - slowdown;
        let p = &mut self.world.players;
        p.anchor_x[n] = self.dribble_target.x;
        p.anchor_y[n] = self.dribble_target.y;
        let dist = self.pos(n).distance(self.dribble_target);
        // Ведение идёт ровным бегом, у точки останавливается.
        mods.urgent[n] = dist > 1.5;
    }

    /// Прогноз пути мяча: точки и времена прихода; для верхового мяча одна точка приземления.
    #[inline(never)]
    fn predict_ball(
        &self,
        points: &mut [Vec2; PREDICT_POINTS],
        times: &mut [f32; PREDICT_POINTS],
    ) -> usize {
        let b = self.world.ball_pos;
        let v = self.world.ball_vel;
        let vh = Vec2::new(v.x, v.y);
        let speed = vh.length();
        let airborne = v.z.abs() > 0.5 || b.z > self.phys.ball.radius_m + 0.3;
        if airborne {
            let g = self.phys.ball.gravity_ms2;
            let t_land = (v.z
                + libm::sqrtf((v.z * v.z + 2.0 * g * (b.z - self.phys.ball.radius_m)).max(0.0)))
                / g;
            let land = Vec2::new(b.x, b.y) + vh * t_land * 0.93;
            points[0] = land;
            times[0] = t_land;
            return 1;
        }
        if speed < SLOW_BALL_MS {
            points[0] = Vec2::new(b.x, b.y);
            times[0] = 0.0;
            return 1;
        }
        let dir = vh / speed;
        let prof = decision::ball_flight(&self.phys, speed, PREDICT_LENGTH_M, PREDICT_POINTS);
        let mut n = 0;
        for k in 0..PREDICT_POINTS {
            if prof.times[k] == f32::MAX {
                break;
            }
            points[k] = Vec2::new(b.x, b.y)
                + dir * (PREDICT_LENGTH_M * (k + 1) as f32 / PREDICT_POINTS as f32);
            times[k] = prof.times[k];
            n = k + 1;
        }
        if n == 0 {
            points[0] = Vec2::new(b.x, b.y);
            times[0] = 0.0;
            n = 1;
        }
        n
    }

    /// Мяч в полёте или ничей: получатель идёт на перехват своего паса, по одному ближайшему
    /// игроку каждой команды бегут к мячу.
    #[inline(never)]
    fn assign_chasers(&mut self, mods: &mut MoveMods) {
        if let BallState::Dead { taker, .. } = self.state {
            let n = taker as usize;
            let p = &mut self.world.players;
            p.anchor_x[n] = self.dead_point.x;
            p.anchor_y[n] = self.dead_point.y;
            p.duty[n] = DUTY_CHASE;
            mods.urgent[n] = true;
            return;
        }
        let mut points = [Vec2::ZERO; PREDICT_POINTS];
        let mut times = [0.0f32; PREDICT_POINTS];
        let m = self.predict_ball(&mut points, &mut times);
        let receiver = match self.state {
            BallState::Flight { receiver, .. } => receiver.map(|r| r as usize),
            _ => None,
        };
        // Для каждого игрока: самая ранняя точка пути, куда он успевает не позже мяча.
        let mut best = [(f32::MAX, Vec2::ZERO, false); 2];
        let mut best_n = [usize::MAX; 2];
        let kicker = match self.state {
            BallState::Flight { kicker, .. } => Some(kicker as usize),
            _ => None,
        };
        for n in 0..PLAYERS {
            if n % SLOTS == 0 && Some(n) != receiver {
                // Вратари участвуют только как получатели и при близком мяче.
                let d = self.pos(n).distance(self.ball2());
                if d > 12.0 {
                    continue;
                }
            }
            if self.lockout[n] > 0 || Some(n) == kicker {
                continue;
            }
            let (pos, vel, body) = (self.pos(n), self.vel(n), &self.world.body[n]);
            let mut found = None;
            for k in 0..m {
                let t = time_to_reach(pos, vel, body, 0.2, points[k]);
                if t <= times[k] || k + 1 == m {
                    found = Some((times[k].max(t), points[k], t <= times[k]));
                    break;
                }
            }
            let Some((t, p, feasible)) = found else {
                continue;
            };
            if Some(n) == receiver {
                let a = &mut self.world.players;
                a.anchor_x[n] = p.x;
                a.anchor_y[n] = p.y;
                a.duty[n] = DUTY_RECEIVE;
                mods.urgent[n] = true;
                continue;
            }
            let team = n / SLOTS;
            // Не выполнимый «успеть» тоже допустим для ничьего мяча: бежим в точку остановки.
            let score = if feasible { t } else { t + 5.0 };
            if score < best[team].0 {
                best[team] = (score, p, feasible);
                best_n[team] = n;
            }
        }
        for team in 0..2 {
            let n = best_n[team];
            if n == usize::MAX {
                continue;
            }
            // Команда с мячом в полёте не гонится за собственным ничьим мячом, если есть получатель.
            if matches!(self.state, BallState::Flight { team: k, .. } if k as usize == team && receiver.is_some())
            {
                continue;
            }
            let a = &mut self.world.players;
            a.anchor_x[n] = best[team].1.x;
            a.anchor_y[n] = best[team].1.y;
            a.duty[n] = DUTY_CHASE;
            mods.urgent[n] = true;
        }
        self.goalkeeper_reaction(mods);
    }

    /// Вратарь бросается к точке, где мяч удара пересечёт линию ворот.
    fn goalkeeper_reaction(&mut self, mods: &mut MoveMods) {
        let Some(sh) = self.shot else { return };
        let half = self.phys.pitch.length_m * 0.5;
        let goal_x = if sh.team == 0 { half } else { -half };
        let (b, v) = (self.world.ball_pos, self.world.ball_vel);
        if v.x * goal_x <= 0.0 {
            return;
        }
        let gk = (1 - sh.team as usize) * SLOTS;
        let reach = self.phys.goal.width_m * 0.5 + SAVE_LATERAL_MARGIN_M;
        let y = (b.y + v.y * (goal_x - b.x) / v.x).clamp(-reach, reach);
        let p = &mut self.world.players;
        p.anchor_x[gk] = goal_x - goal_x.signum() * SAVE_GOAL_LINE_OFFSET_M;
        p.anchor_y[gk] = y;
        p.duty[gk] = DUTY_CHASE;
        mods.urgent[gk] = true;
    }

    // ---------- хэш ----------

    /// FNV-1a по битам состояния игроков, мяча, владения и счёта.
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
        let code = match self.state {
            BallState::Carried { team, slot } => 1.0 + team as f32 * 100.0 + slot as f32,
            BallState::Flight { team, kicker, .. } => 2000.0 + team as f32 * 100.0 + kicker as f32,
            BallState::Loose { last_team } => 3000.0 + last_team as f32,
            BallState::Dead { team, ticks, .. } => 4000.0 + team as f32 * 100.0 + ticks as f32,
        };
        mix(code);
        for g in self.stats.goals {
            mix(g as f32);
        }
        h
    }
}

/// Снимок положений в системе команды для исполнения паса после решения.
struct CtxSnapshot {
    mates_pos: [Vec2; SLOTS],
    mates_vel: [Vec2; SLOTS],
    opp_pos: [Vec2; SLOTS],
    opp_vel: [Vec2; SLOTS],
}

fn ctx_snapshot(
    a: &[Vec2; SLOTS],
    b: &[Vec2; SLOTS],
    c: &[Vec2; SLOTS],
    d: &[Vec2; SLOTS],
) -> CtxSnapshot {
    CtxSnapshot {
        mates_pos: *a,
        mates_vel: *b,
        opp_pos: *c,
        opp_vel: *d,
    }
}

fn decision_dist_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

fn decision_angle_sigma(ctx: &Ctx, skill: f32, pressure: f32) -> f32 {
    decision::angle_sigma(ctx, skill, pressure)
}

fn decision_speed_sigma(ctx: &Ctx, skill: f32, pressure: f32) -> f32 {
    decision::speed_sigma(ctx, skill, pressure)
}

/// Прогон матча без визуализатора: команды из одинаковых игроков. Возвращает хэш состояния.
pub fn run_match(
    seed: u64,
    ticks: u32,
    cfg: &Config,
    styles: [&str; 2],
    formations: [&str; 2],
) -> Result<u64, String> {
    let mut teams = Vec::with_capacity(2);
    for k in 0..2 {
        let style = cfg.styles.get(styles[k]).ok_or("стиль не найден")?;
        teams.push(TeamSetup::uniform(
            cfg,
            formations[k],
            style,
            cfg.sandbox.uniform_attr,
        )?);
    }
    let [a, b]: [TeamSetup; 2] = teams.try_into().map_err(|_| "две команды")?;
    let mut m = LiveMatch::new(seed, cfg, [a, b], crate::sink::NoopSink);
    m.run(ticks);
    Ok(m.state_hash())
}
