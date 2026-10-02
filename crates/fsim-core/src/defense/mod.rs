//! Оборона без мяча: распределение обязанностей венгерским алгоритмом, прессинг с триггерами,
//! тени прикрытия, страховка, опека и контрпрессинг (`docs/04-off-ball.md` §2, §5).
//!
//! Все расчёты ведутся в системе обороняющейся команды: `X` от её ворот, `Y` положительный
//! справа от её атакующего направления. Игроки: слот 0 вратарь, слоты 1–10 полевые.

pub mod hungarian;

use crate::arrival::time_to_reach;
use crate::config::{DefenseConfig, PressTrigger};
use crate::data::Position;
use crate::data::STARTERS;
use crate::positioning::lerp;
use crate::positioning::team::{Group, TeamSetup};
use glam::Vec2;
use hungarian::{solve, CostMatrix, MAX_COLS, MAX_ROWS};

pub const DUTY_NONE: u8 = 0;
pub const DUTY_ZONAL: u8 = 1;
pub const DUTY_PRESS: u8 = 2;
pub const DUTY_SUPPORT: u8 = 3;
pub const DUTY_COVER: u8 = 4;
pub const DUTY_MARK: u8 = 5;
pub const DUTY_COUNTER: u8 = 6;
/// Владелец мяча в атакующей команде (для отладки).
pub const DUTY_OWNER: u8 = 7;
/// Получатель паса и игрок, бегущий к ничьему мячу (отладка).
pub const DUTY_RECEIVE: u8 = 8;
pub const DUTY_CHASE: u8 = 9;
/// Нет ссылки на другого игрока.
pub const NO_REF: u8 = 255;

const OUTFIELD: usize = STARTERS - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub kind: u8,
    /// Зональная: слот формации. Поддержка: ранг ближайшего к владельцу соперника.
    /// Опека: слот соперника. Контрпрессинг: номер места в кольце.
    pub reference: u8,
}

impl Assignment {
    const ZONAL_NONE: Assignment = Assignment {
        kind: DUTY_NONE,
        reference: NO_REF,
    };
}

/// Итог за тик для обороняющейся команды.
#[derive(Clone, Debug)]
pub struct Plan {
    pub kind: [u8; STARTERS],
    /// Слот игрока соперника, к которому относится обязанность, или `NO_REF`.
    pub target_player: [u8; STARTERS],
    /// Целевая точка в системе обороняющейся команды.
    pub target: [Vec2; STARTERS],
    pub press_on: bool,
    pub counter_on: bool,
}

impl Plan {
    pub fn new() -> Plan {
        Plan {
            kind: [DUTY_NONE; STARTERS],
            target_player: [NO_REF; STARTERS],
            target: [Vec2::ZERO; STARTERS],
            press_on: false,
            counter_on: false,
        }
    }

    /// Сколько игроков заняты активной обороной (прессинг, поддержка, страховка, контрпрессинг).
    pub fn pressers(&self) -> usize {
        self.kind
            .iter()
            .filter(|&&k| matches!(k, DUTY_PRESS | DUTY_SUPPORT | DUTY_COVER | DUTY_COUNTER))
            .count()
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

/// Данные игроков обороняющейся команды и атакующей в системе обороняющейся.
pub struct Inputs<'a> {
    pub tick: u32,
    pub tick_hz: u32,
    pub pitch_length: f32,
    pub pitch_width: f32,
    pub ball: Vec2,
    pub ball_vel: Vec2,
    /// Слот владельца мяча в атакующей команде.
    pub owner: usize,
    /// Направление взгляда и скорость владельца в системе обороняющейся команды.
    pub owner_facing: Vec2,
    pub owner_vel: Vec2,
    pub def_pos: &'a [Vec2; STARTERS],
    pub def_vel: &'a [Vec2; STARTERS],
    pub att_pos: &'a [Vec2; STARTERS],
    /// Зональные якоря обороняющейся команды (из `positioning`).
    pub anchors: &'a [Vec2; STARTERS],
    pub attackers: &'a TeamSetup,
    /// Владение только что перешло к атакующей команде.
    pub lost_possession: bool,
    /// Правила матча: оборона встречает владельца у своих ворот независимо от стиля.
    pub engage: bool,
}

#[derive(Clone, Debug)]
pub struct DefenseState {
    assign: [Assignment; STARTERS],
    next_assign_tick: u32,
    press_until: u32,
    counter_until: u32,
    last_press_on: bool,
    last_counter_on: bool,
    last_owner: usize,
}

impl Default for DefenseState {
    fn default() -> Self {
        Self::new()
    }
}

impl DefenseState {
    pub fn new() -> DefenseState {
        DefenseState {
            assign: std::array::from_fn(|i| {
                if i == 0 {
                    Assignment::ZONAL_NONE
                } else {
                    Assignment {
                        kind: DUTY_ZONAL,
                        reference: i as u8,
                    }
                }
            }),
            next_assign_tick: 0,
            press_until: 0,
            counter_until: 0,
            last_press_on: false,
            last_counter_on: false,
            last_owner: usize::MAX,
        }
    }

    /// Занятые зональные места игроков (слот формации) по слотам игроков; `None` у остальных.
    pub fn zonal_slots(&self) -> [Option<u8>; STARTERS] {
        std::array::from_fn(|i| {
            let a = self.assign[i];
            (a.kind == DUTY_ZONAL && i != 0).then_some(a.reference)
        })
    }

    /// Один тик обороны: триггеры, при необходимости пересчёт назначений, цели игроков.
    pub fn step(&mut self, cfg: &DefenseConfig, team: &TeamSetup, inp: &Inputs, plan: &mut Plan) {
        let st = &team.style;
        let hz = inp.tick_hz as f32;

        // ---- триггеры и режим прессинга ----
        let zone_x = lerp(cfg.press.zone_min_m, cfg.press.zone_max_m, st.block_height);
        let engaged = inp.engage
            && inp.ball.x <= cfg.press.engage_zone_m
            && inp
                .def_pos
                .iter()
                .skip(1)
                .any(|p| p.distance(inp.ball) <= cfg.press.engage_radius_m);
        let zone_ok = inp.ball.x <= zone_x;
        if zone_ok && self.trigger_fired(cfg, team, inp) {
            let dur = lerp(
                cfg.press.duration_min_s,
                cfg.press.duration_max_s,
                st.press_intensity,
            );
            self.press_until = self.press_until.max(inp.tick + (dur * hz) as u32);
        }
        let always = st.press_intensity >= cfg.press.always_threshold;
        let press_on = engaged || zone_ok && (always || inp.tick < self.press_until);
        if inp.lost_possession && st.counter_press >= cfg.counter_press.min {
            let dur = lerp(
                cfg.counter_press.duration_min_s,
                cfg.counter_press.duration_max_s,
                st.counter_press,
            );
            self.counter_until = inp.tick + (dur * hz) as u32;
        }
        let counter_on = inp.tick < self.counter_until;
        let active = press_on || counter_on;

        // ---- пересчёт назначений: по таймеру и по событиям ----
        let forced = inp.lost_possession
            || press_on != self.last_press_on
            || counter_on != self.last_counter_on
            || inp.owner != self.last_owner && active;
        if forced || inp.tick >= self.next_assign_tick {
            self.reassign(cfg, team, inp, active, counter_on);
            self.next_assign_tick = inp.tick + (cfg.period_s * hz).max(1.0) as u32;
        }
        self.last_press_on = press_on;
        self.last_counter_on = counter_on;
        self.last_owner = inp.owner;

        self.fill_plan(cfg, inp, plan);
        plan.press_on = press_on;
        plan.counter_on = counter_on;
    }

    fn trigger_fired(&self, cfg: &DefenseConfig, team: &TeamSetup, inp: &Inputs) -> bool {
        let p = &cfg.press;
        team.style.press_triggers.iter().any(|t| match t {
            PressTrigger::BackPass => inp.ball_vel.x > p.back_pass_speed_ms,
            PressTrigger::Touchline => inp.ball.y.abs() > inp.pitch_width * 0.5 - p.touchline_m,
            // Владелец смотрит от ворот, которые атакует: в системе обороны это +X.
            PressTrigger::BackToGoal => inp.owner_facing.x > 0.5,
            PressTrigger::PassToKeeper => {
                inp.attackers.slots[inp.owner].group == Group::Keeper
                    && inp.ball.x > inp.pitch_length - p.keeper_zone_m
            }
            // Плохой приём появится вместе с приёмом мяча на M4.
            PressTrigger::BadTouch => false,
        })
    }

    /// Целевые точки особых обязанностей в текущей геометрии.
    fn press_target(&self, cfg: &DefenseConfig, inp: &Inputs) -> Vec2 {
        let v = inp.owner_vel;
        // Упреждение: встречаем владельца там, где он будет.
        let o = inp.att_pos[inp.owner] + v * cfg.press.lead_s;
        // Владелец идёт к нашим воротам: встаём на его пути (между ним и воротами), а не
        // догоняем сзади.
        if v.x < -cfg.press.contain_speed_ms {
            return o + (-o).normalize_or_zero() * cfg.press.cut_offset_m.max(cfg.press.contact_m);
        }
        // Иначе встаём на линию ближайшего паса: к ближайшему партнёру владельца.
        let mut best = (f32::MAX, Vec2::X);
        for (i, &p) in inp.att_pos.iter().enumerate() {
            if i == inp.owner || inp.attackers.slots[i].group == Group::Keeper {
                continue;
            }
            let d = o.distance(p);
            if d < best.0 {
                best = (d, (p - o) / d.max(1e-3));
            }
        }
        // Не ближе контактной дистанции и не дальше смещения на линии паса.
        o + best.1 * cfg.press.cut_offset_m.max(cfg.press.contact_m)
    }

    /// Ближайшие к владельцу партнёры по возрастанию расстояния (без самого владельца).
    fn nearest_options(inp: &Inputs, out: &mut [usize; STARTERS]) -> usize {
        let o = inp.att_pos[inp.owner];
        let mut n = 0;
        for i in 0..STARTERS {
            if i != inp.owner {
                out[n] = i;
                n += 1;
            }
        }
        out[..n].sort_by(|&a, &b| {
            o.distance_squared(inp.att_pos[a])
                .partial_cmp(&o.distance_squared(inp.att_pos[b]))
                .unwrap()
        });
        n
    }

    fn support_target(cfg: &DefenseConfig, inp: &Inputs, rank: usize) -> Vec2 {
        let mut opts = [0usize; STARTERS];
        let n = Self::nearest_options(inp, &mut opts);
        let o = inp.att_pos[inp.owner];
        let opp = inp.att_pos[opts[rank.min(n.saturating_sub(1))]];
        let len = o.distance(opp).max(1e-3);
        let along = (cfg.press.support_lane_frac * len)
            .max(cfg.press.support_min_m)
            .min(len);
        o + (opp - o) / len * along
    }

    fn counter_target(cfg: &DefenseConfig, inp: &Inputs, ring: usize, count: usize) -> Vec2 {
        let o = inp.att_pos[inp.owner];
        let to_goal = (-o).normalize_or_zero();
        let base = libm::atan2f(to_goal.y, to_goal.x);
        let spread = cfg.counter_press.ring_spread_deg.to_radians();
        let off = if count > 1 {
            (ring as f32 / (count - 1) as f32 - 0.5) * 2.0 * spread
        } else {
            0.0
        };
        o + Vec2::new(libm::cosf(base + off), libm::sinf(base + off))
            * cfg.counter_press.ring_radius_m
    }

    fn mark_target(cfg: &DefenseConfig, inp: &Inputs, opp: usize) -> Vec2 {
        let m = inp.att_pos[opp];
        // Между подопечным и своими воротами (в нуле системы обороны).
        m + (-m).normalize_or_zero() * cfg.marking.goalside_m
    }

    fn cover_target(&self, cfg: &DefenseConfig, inp: &Inputs, presser: Vec2) -> Vec2 {
        let _ = inp;
        presser + (-presser).normalize_or_zero() * cfg.press.cover_back_m
    }

    fn reassign(
        &mut self,
        cfg: &DefenseConfig,
        team: &TeamSetup,
        inp: &Inputs,
        active: bool,
        counter_on: bool,
    ) {
        let st = &team.style;
        let a = &cfg.assign;

        // ---- набор особых обязанностей ----
        let mut specials = [Assignment::ZONAL_NONE; MAX_COLS - OUTFIELD];
        let mut n_spec = 0usize;
        if active {
            specials[n_spec] = Assignment {
                kind: DUTY_PRESS,
                reference: NO_REF,
            };
            n_spec += 1;
            let extras = if counter_on {
                (st.counter_press * (cfg.counter_press.players_max as f32 - 1.0)).round() as usize
            } else {
                (st.press_intensity * cfg.press.max_support as f32).round() as usize
            };
            for r in 0..extras {
                let kind = if counter_on {
                    DUTY_COUNTER
                } else {
                    DUTY_SUPPORT
                };
                specials[n_spec] = Assignment {
                    kind,
                    reference: r as u8,
                };
                n_spec += 1;
            }
            if st.press_intensity >= cfg.press.cover_threshold {
                specials[n_spec] = Assignment {
                    kind: DUTY_COVER,
                    reference: NO_REF,
                };
                n_spec += 1;
            }
        }
        // Опека: самые опасные соперники в зоне.
        let n_marks = (st.marking * cfg.marking.max_marks as f32).round() as usize;
        if n_marks > 0 {
            let depth = lerp(
                cfg.marking.zone_depth_zonal_m,
                cfg.marking.zone_depth_man_m,
                st.marking,
            );
            let mut order = [0usize; STARTERS];
            let mut n = 0;
            for i in 0..STARTERS {
                if i != inp.owner
                    && inp.attackers.slots[i].group != Group::Keeper
                    && inp.att_pos[i].x <= depth
                {
                    order[n] = i;
                    n += 1;
                }
            }
            order[..n].sort_by(|&x, &y| inp.att_pos[x].x.partial_cmp(&inp.att_pos[y].x).unwrap());
            for &opp in order[..n.min(n_marks)].iter() {
                if n_spec < OUTFIELD - 1 {
                    specials[n_spec] = Assignment {
                        kind: DUTY_MARK,
                        reference: opp as u8,
                    };
                    n_spec += 1;
                }
            }
        }
        n_spec = n_spec.min(OUTFIELD - 1);
        let counter_n = specials[..n_spec]
            .iter()
            .filter(|s| s.kind == DUTY_COUNTER)
            .count()
            + 1;

        // ---- цели особых обязанностей на момент пересчёта ----
        let press_t = self.press_target(cfg, inp);
        let mut spec_target = [Vec2::ZERO; MAX_COLS - OUTFIELD];
        for (c, s) in specials[..n_spec].iter().enumerate() {
            spec_target[c] = match s.kind {
                DUTY_PRESS => press_t,
                DUTY_SUPPORT => Self::support_target(cfg, inp, s.reference as usize),
                DUTY_COUNTER => Self::counter_target(cfg, inp, s.reference as usize, counter_n),
                DUTY_COVER => self.cover_target(cfg, inp, press_t),
                _ => Self::mark_target(cfg, inp, s.reference as usize),
            };
        }

        // ---- матрица стоимости 10 × (S + 10) ----
        let n_cols = n_spec + OUTFIELD;
        let mut cm = CostMatrix::new(OUTFIELD, n_cols);
        let stick = lerp(a.stick_max_s, a.stick_min_s, team.communication());
        let cb_may_press = st.press_intensity >= a.cb_press_threshold;
        for r in 0..OUTFIELD {
            let slot = r + 1;
            let (pos, vel) = (inp.def_pos[slot], inp.def_vel[slot]);
            let body = &team.body[slot];
            let group = team.slots[slot].group;
            let prev = self.assign[slot];
            for (c, s) in specials[..n_spec].iter().enumerate() {
                let mut cost = time_to_reach(pos, vel, body, a.react_s, spec_target[c]);
                if matches!(
                    s.kind,
                    DUTY_PRESS | DUTY_SUPPORT | DUTY_COVER | DUTY_COUNTER
                ) && group == Group::Back
                {
                    cost += if team.slots[slot].pos == Position::CB && !cb_may_press {
                        a.cb_leave_cost_s
                    } else {
                        a.back_leave_cost_s
                    };
                }
                if prev == *s {
                    cost -= stick;
                }
                cm.set(r, c, cost - a.special_bonus_s);
            }
            for j in 1..STARTERS {
                let mut cost = time_to_reach(pos, vel, body, a.react_s, inp.anchors[j]);
                if j != slot {
                    cost += a.swap_base_s;
                    if team.slots[j].group != group {
                        cost += a.group_mismatch_s;
                    }
                }
                if prev.kind == DUTY_ZONAL && prev.reference as usize == j {
                    cost -= stick;
                }
                cm.set(r, n_spec + j - 1, cost);
            }
        }
        let mut col = [0usize; MAX_ROWS];
        solve(&cm, &mut col);
        for (r, &c) in col.iter().take(OUTFIELD).enumerate() {
            self.assign[r + 1] = if c < n_spec {
                specials[c]
            } else {
                Assignment {
                    kind: DUTY_ZONAL,
                    reference: (c - n_spec + 1) as u8,
                }
            };
        }
        self.assign[0] = Assignment::ZONAL_NONE;
    }

    /// Целевые точки по текущим назначениям и текущей геометрии.
    fn fill_plan(&self, cfg: &DefenseConfig, inp: &Inputs, plan: &mut Plan) {
        // Число мест в кольце контрпрессинга и ранги поддержки считаем по назначениям.
        let counter_n = self
            .assign
            .iter()
            .filter(|a| a.kind == DUTY_COUNTER)
            .count()
            + 1;
        let press_t = self.press_target(cfg, inp);
        for slot in 0..STARTERS {
            let a = self.assign[slot];
            plan.kind[slot] = a.kind;
            plan.target_player[slot] = NO_REF;
            plan.target[slot] = match a.kind {
                DUTY_ZONAL if slot != 0 => inp.anchors[a.reference as usize],
                DUTY_PRESS => {
                    plan.target_player[slot] = inp.owner as u8;
                    press_t
                }
                DUTY_SUPPORT => {
                    plan.target_player[slot] = inp.owner as u8;
                    Self::support_target(cfg, inp, a.reference as usize)
                }
                DUTY_COUNTER => {
                    plan.target_player[slot] = inp.owner as u8;
                    Self::counter_target(cfg, inp, a.reference as usize, counter_n)
                }
                DUTY_COVER => {
                    plan.target_player[slot] = inp.owner as u8;
                    self.cover_target(cfg, inp, press_t)
                }
                DUTY_MARK => {
                    plan.target_player[slot] = a.reference;
                    Self::mark_target(cfg, inp, a.reference as usize)
                }
                _ => inp.anchors[slot],
            };
        }
    }
}

/// Владелец мяча в атакующей команде: ближайший к мячу, прежний сохраняется, пока другой
/// не ближе на запас `margin` (гистерезис против дребезга).
pub fn select_owner(
    prev: Option<usize>,
    att_pos: &[Vec2; STARTERS],
    ball: Vec2,
    margin: f32,
) -> usize {
    let mut best = (f32::MAX, 0usize);
    for (i, p) in att_pos.iter().enumerate() {
        let d = p.distance(ball);
        if d < best.0 {
            best = (d, i);
        }
    }
    match prev {
        Some(p) if att_pos[p].distance(ball) <= best.0 + margin => p,
        _ => best.1,
    }
}
