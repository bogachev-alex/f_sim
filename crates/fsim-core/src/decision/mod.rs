//! Решения владельца мяча (`docs/03-decisions.md` §1–3, §6): кандидаты, поэтапная оценка,
//! полезность, softmax и гистерезис. Все расчёты в системе команды владельца: `X` от своих
//! ворот к воротам соперника. Без аллокаций.

mod candidates;
mod eval;
mod shot;

pub use eval::{
    angle_sigma, ball_flight, pass_launch, pass_terms, reception_probability, speed_sigma,
    FlightProfile, Launch, PassTerms, LOFT_RECEIVE_SPEED_FACTOR,
};
pub use shot::{goal_angle, xg};

use crate::config::{DecisionConfig, PhysicsConfig};
use crate::positioning::team::TeamSetup;
use crate::rng::Rng;
use crate::value::Xt;
use glam::Vec2;

pub const SLOTS: usize = 11;
/// Предел числа кандидатов этапа 1 (`docs/03-decisions.md` §3: до 60).
pub const MAX_CANDIDATES: usize = 64;
pub const NO_SLOT: u8 = 255;
/// Дистанция до ворот, с которой удар считается дальним, м.
pub const LONG_SHOT_FROM_M: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Пас по земле в ноги партнёру.
    PassFeet,
    /// Пас по земле в пространство перед партнёром.
    PassSpace,
    /// Пас за спину защитникам соперника.
    Through,
    /// Длинный верховой пас партнёру.
    Long,
    /// Ведение мяча.
    Dribble,
    /// Укрывание и удержание.
    Shield,
    /// Вынос в сторону фланга.
    Clearance,
    /// Удар по воротам.
    Shot,
}

impl Kind {
    pub fn is_pass(self) -> bool {
        matches!(
            self,
            Kind::PassFeet | Kind::PassSpace | Kind::Through | Kind::Long | Kind::Clearance
        )
    }

    /// Верховой пас: мяч идёт по воздуху.
    pub fn is_lofted(self) -> bool {
        matches!(self, Kind::Long | Kind::Clearance)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub kind: Kind,
    /// Партнёр-адресат или `NO_SLOT`.
    pub slot: u8,
    /// Номер направления ведения (для гистерезиса).
    pub dir: u8,
    /// Целевая точка в системе команды.
    pub point: Vec2,
    /// Оценка этапа 1.
    pub h: f32,
    /// Вероятность успеха, ценность после действия, цена потери, полезность (этап 2).
    pub p: f32,
    pub v: f32,
    pub c: f32,
    pub u: f32,
}

impl Candidate {
    pub const EMPTY: Candidate = Candidate {
        kind: Kind::Shield,
        slot: NO_SLOT,
        dir: 0,
        point: Vec2::ZERO,
        h: 0.0,
        p: 1.0,
        v: 0.0,
        c: 0.0,
        u: 0.0,
    };
}

/// Положения игроков и параметры команд для решения.
pub struct Ctx<'a> {
    pub cfg: &'a DecisionConfig,
    pub phys: &'a PhysicsConfig,
    pub xt: &'a Xt,
    pub me: usize,
    pub mates_pos: &'a [Vec2; SLOTS],
    pub mates_vel: &'a [Vec2; SLOTS],
    pub opp_pos: &'a [Vec2; SLOTS],
    pub opp_vel: &'a [Vec2; SLOTS],
    pub team: &'a TeamSetup,
    pub opp: &'a TeamSetup,
    /// Сколько секунд владелец уже держит мяч.
    pub held_s: f32,
}

impl Ctx<'_> {
    pub fn length(&self) -> f32 {
        self.phys.pitch.length_m
    }

    pub fn width(&self) -> f32 {
        self.phys.pitch.width_m
    }

    /// Давление соперников в точке: сумма `1 − d/R` по соперникам в радиусе, нормированная.
    pub fn pressure_at(&self, p: Vec2) -> f32 {
        let r = self.cfg.pressure.radius_m;
        let sum: f32 = self
            .opp_pos
            .iter()
            .map(|o| (1.0 - o.distance(p) / r).max(0.0))
            .sum();
        (sum / PRESSURE_NORM).min(1.0)
    }
}

/// Сумма вкладов, которая считается полным давлением (около двух соперников вплотную).
const PRESSURE_NORM: f32 = 1.5;

/// Гистерезис ведения: направление держится, пока другое не лучше на `commit.margin` или не
/// истёк `commit.time_s`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Commit {
    pub active: bool,
    pub dir: u8,
    pub until_tick: u32,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub chosen: Candidate,
    /// Кандидаты после этапа 2 по убыванию полезности.
    pub top: [Candidate; 8],
    pub n_top: usize,
    pub temperature: f32,
    pub pressure: f32,
    /// Сколько кандидатов построено на этапе 0.
    pub n_generated: usize,
}

/// Решение владельца мяча: этапы 1 и 2, softmax.
#[inline(never)]
pub fn decide(ctx: &Ctx, rng: &mut Rng, commit: &mut Commit, tick: u32, tick_hz: u32) -> Decision {
    let cfg = ctx.cfg;
    let me_pos = ctx.mates_pos[ctx.me];
    let pressure = ctx.pressure_at(me_pos);

    // Этап 0 и 1: кандидаты с быстрой оценкой.
    let mut all = [Candidate::EMPTY; MAX_CANDIDATES];
    let n = candidates::generate(ctx, pressure, &mut all);
    let keep1 = (cfg.candidates.stage1_keep as usize).min(n);
    all[..n].sort_unstable_by(|a, b| b.h.partial_cmp(&a.h).unwrap());
    // Резерв: лучшие безопасные кандидаты (пас в ноги, ведение, укрывание) вместо хвоста
    // списка рискованных пасов вперёд.
    let reserve = (cfg.candidates.stage1_reserve as usize).min(keep1);
    let mut taken = keep1 - reserve;
    for i in keep1 - reserve..n {
        if taken >= keep1 {
            break;
        }
        if matches!(all[i].kind, Kind::PassFeet | Kind::Dribble | Kind::Shield) {
            all.swap(taken, i);
            taken += 1;
        }
    }

    // Этап 2: точная оценка оставшихся.
    let mut top = [Candidate::EMPTY; 8];
    let mut m = 0;
    for c in all[..keep1].iter_mut() {
        eval::evaluate(ctx, pressure, c);
        // Прежнее направление ведения получает бонус гистерезиса.
        if c.kind == Kind::Dribble
            && commit.active
            && tick < commit.until_tick
            && c.dir == commit.dir
        {
            c.u += cfg.commit.margin;
        }
        if m < top.len() {
            top[m] = *c;
            m += 1;
        } else if c.u > top[m - 1].u {
            top[m - 1] = *c;
        }
        top[..m].sort_unstable_by(|a, b| b.u.partial_cmp(&a.u).unwrap());
    }
    let keep2 = (cfg.candidates.stage2_keep as usize).min(m).max(1);

    // Softmax по лучшим кандидатам. Температура растёт при слабых решениях, низкой морали,
    // усталости и давлении на нервном игроке.
    let mental = &ctx.team.mental[ctx.me];
    let s = &cfg.softmax;
    let t = (s.base
        + s.k_decisions * (1.0 - mental.decisions)
        + s.k_morale * (0.5 - s.morale)
        + s.k_fatigue * s.fatigue
        + s.k_pressure * pressure * (1.0 - mental.composure))
        .max(s.base * 0.25);
    let best = top[0].u;
    let mut weights = [0.0f32; 8];
    let mut total = 0.0;
    for (w, c) in weights.iter_mut().zip(&top[..keep2]) {
        *w = libm::expf((c.u - best) / t);
        total += *w;
    }
    let mut pick = rng.next_f32() * total;
    let mut chosen = top[0];
    for (w, c) in weights.iter().zip(&top[..keep2]) {
        if pick < *w {
            chosen = *c;
            break;
        }
        pick -= *w;
    }
    // Запоминаем выбор ведения.
    if chosen.kind == Kind::Dribble {
        if !(commit.active && commit.dir == chosen.dir && tick < commit.until_tick) {
            commit.dir = chosen.dir;
            commit.until_tick = tick + (cfg.commit.time_s * tick_hz as f32) as u32;
        }
        commit.active = true;
    } else {
        commit.active = false;
    }
    Decision {
        chosen,
        top,
        n_top: m,
        temperature: t,
        pressure,
        n_generated: n,
    }
}

/// Кандидаты этапа 1 с точной оценкой и разбором вероятности пасов. Только для отладки.
pub fn explain(ctx: &Ctx) -> Vec<(Candidate, PassTerms)> {
    let me_pos = ctx.mates_pos[ctx.me];
    let pressure = ctx.pressure_at(me_pos);
    let mut all = [Candidate::EMPTY; MAX_CANDIDATES];
    let n = candidates::generate(ctx, pressure, &mut all);
    all[..n].sort_unstable_by(|a, b| b.h.partial_cmp(&a.h).unwrap());
    let keep = (ctx.cfg.candidates.stage1_keep as usize).min(n);
    all[..keep]
        .iter_mut()
        .map(|c| {
            eval::evaluate(ctx, pressure, c);
            let terms = if c.kind.is_pass() {
                eval::pass_terms(ctx, pressure, c)
            } else {
                PassTerms::default()
            };
            (*c, terms)
        })
        .collect()
}
