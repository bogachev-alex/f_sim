//! Точная оценка кандидатов этапа 2: физика полёта мяча, перехват по модели прибытия,
//! приём, ценность xT × контроль, цена потери (`docs/03-decisions.md` §2, §3, §7).

use super::{Candidate, Ctx, Kind, LONG_SHOT_FROM_M, NO_SLOT};
use crate::arrival::time_to_reach;
use crate::config::PhysicsConfig;
use crate::positioning::lerp;
use glam::Vec2;

const MAX_POINTS: usize = 32;
/// Шагов интегрирования качения мяча.
const ROLL_STEPS: usize = 16;
/// Скорость, ниже которой мяч считается остановившимся.
const BALL_STOP_MS: f32 = 0.5;
/// Запас на сопротивление воздуха при расчёте дальности верхового паса.
const LOFT_DRAG_MARGIN: f32 = 1.06;
/// Горизонтальная скорость верхового мяча к концу полёта относительно начальной.
const LOFT_SPEED_KEEP: f32 = 0.95;
/// Верховой мяч принимают хуже: поправка к навыку контроля получателя.
const LOFT_CONTROL_PENALTY: f32 = 0.08;
/// У верхового паса время полёта позволяет получателю подстроиться: радиус приёма больше.
const LOFT_CATCH_FACTOR: f32 = 2.5;
/// Верховой мяч принимают у земли после падения: скорость приёма относительно начальной.
pub const LOFT_RECEIVE_SPEED_FACTOR: f32 = 0.4;
/// Начало верхового паса низко над землёй: на этой длине соперник рядом с пасующим ещё дотягивается.
const LOFT_EARLY_LANE_M: f32 = 6.0;
const LOFT_EARLY_POINTS: usize = 3;
/// Допуск по времени для приёма: получатель может опоздать к мячу на эту величину.
const RECEIVE_SLACK_S: f32 = 0.2;
/// Реакция получателя на начало движения мяча.
const RECEIVER_REACT_S: f32 = 0.15;
/// Дистанция, с которой удар считается дальним, м, и нейтральный вес дальних ударов в стиле.
const LONG_SHOT_BASE_WEIGHT: f32 = 0.15;
/// Доля риска потери мяча при укрывании на полное давление.
const SHIELD_LOSS_AT_FULL_PRESSURE: f32 = 0.35;

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-x))
}

/// Профиль качения мяча по земле: время достижения точек вдоль пути.
#[derive(Clone, Debug)]
pub struct FlightProfile {
    /// Время (с) до `k`-й точки пути, `k = 1..=n`; `f32::MAX`, если мяч остановится раньше.
    pub times: [f32; MAX_POINTS],
    pub n: usize,
    /// Скорость мяча в конце пути.
    pub v_arrive: f32,
    /// Время до конца пути.
    pub t_end: f32,
}

/// Качение с начальной скоростью `v0` на расстояние `s` с сопротивлением воздуха и трением.
/// Точки пути делят `s` на `n` равных частей.
pub fn ball_flight(phys: &PhysicsConfig, v0: f32, s: f32, n: usize) -> FlightProfile {
    let n = n.clamp(1, MAX_POINTS - 1);
    let (k, a_r) = (phys.ball.air_drag_per_m, phys.ball.rolling_decel_ms2);
    let ds = s / ROLL_STEPS as f32;
    let mut times = [f32::MAX; MAX_POINTS];
    let (mut t, mut v) = (0.0f32, v0);
    let mut stopped = false;
    for step in 1..=ROLL_STEPS {
        if stopped {
            break;
        }
        let v2 = v * v - 2.0 * (a_r + k * v * v) * ds;
        if v2 <= BALL_STOP_MS * BALL_STOP_MS {
            stopped = true;
            break;
        }
        let v_next = libm::sqrtf(v2);
        t += ds / (0.5 * (v + v_next));
        v = v_next;
        // Отметка точек, кратных шагу.
        if (step * n).is_multiple_of(ROLL_STEPS) {
            times[step * n / ROLL_STEPS - 1] = t;
        }
    }
    FlightProfile {
        times,
        n,
        v_arrive: if stopped { 0.0 } else { v },
        t_end: if stopped { f32::MAX } else { t },
    }
}

/// Идеальный запуск паса без ошибки исполнения.
#[derive(Clone, Copy, Debug)]
pub struct Launch {
    /// Горизонтальная скорость и вертикальная составляющая, м/с.
    pub speed_h: f32,
    pub vz: f32,
    /// Время до цели, с.
    pub flight_time: f32,
}

/// Скорость запуска, при которой мяч дойдёт до цели с нужной скоростью приёма (по земле) или
/// приземлится у цели (верховой).
pub fn pass_launch(ctx: &Ctx, kind: Kind, from: Vec2, to: Vec2) -> Launch {
    let pm = &ctx.cfg.pass;
    let s = from.distance(to).max(0.5);
    if kind.is_lofted() {
        let theta = pm.loft_deg.to_radians();
        let g = ctx.phys.ball.gravity_ms2;
        let v = libm::sqrtf(s * g / libm::sinf(2.0 * theta)) * LOFT_DRAG_MARGIN;
        let v = v.min(ctx.phys.ball.shot_speed_max_ms);
        let vh = v * libm::cosf(theta);
        return Launch {
            speed_h: vh,
            vz: v * libm::sinf(theta),
            flight_time: s / (vh * LOFT_SPEED_KEEP),
        };
    }
    // Обратный расчёт качения: от скорости приёма к начальной.
    let (k, a_r) = (
        ctx.phys.ball.air_drag_per_m,
        ctx.phys.ball.rolling_decel_ms2,
    );
    let ds = s / ROLL_STEPS as f32;
    let mut v2 = pm.receive_speed_ms * pm.receive_speed_ms;
    for _ in 0..ROLL_STEPS {
        v2 += 2.0 * (a_r + k * v2) * ds;
    }
    let v0 = libm::sqrtf(v2).clamp(pm.speed_min_ms, pm.speed_max_ms);
    let prof = ball_flight(ctx.phys, v0, s, 8);
    Launch {
        speed_h: v0,
        vz: 0.0,
        flight_time: prof.t_end,
    }
}

/// Навык исполнения паса данного вида у владельца мяча.
fn passer_skill(ctx: &Ctx, kind: Kind) -> f32 {
    let s = &ctx.team.skills[ctx.me];
    match kind {
        Kind::PassFeet | Kind::PassSpace => s.short_pass,
        Kind::Through => 0.5 * (s.short_pass + s.long_pass),
        _ => s.long_pass,
    }
}

/// Стандартное отклонение направления паса, рад, с учётом давления на пасующего.
pub fn angle_sigma(ctx: &Ctx, skill: f32, pressure: f32) -> f32 {
    let pm = &ctx.cfg.pass;
    let base = lerp(pm.angle_sigma_worst_deg, pm.angle_sigma_best_deg, skill);
    let press = 1.0 + pm.pressure_gain * pressure * (1.0 - ctx.team.skills[ctx.me].press_resist);
    (base * press).to_radians()
}

/// Относительное стандартное отклонение силы паса.
pub fn speed_sigma(ctx: &Ctx, skill: f32, pressure: f32) -> f32 {
    let pm = &ctx.cfg.pass;
    let base = lerp(pm.speed_sigma_worst, pm.speed_sigma_best, skill);
    base * (1.0 + pm.pressure_gain * pressure * (1.0 - ctx.team.skills[ctx.me].press_resist))
}

/// Вероятность приёма мяча игроком: логистика от навыка контроля, относительной скорости и
/// давления (`DecisionConfig::control`).
pub fn reception_probability(ctx: &Ctx, control_skill: f32, rel_speed: f32, pressure: f32) -> f32 {
    let c = &ctx.cfg.control;
    sigmoid(
        c.base + c.skill_gain * control_skill
            - c.speed_cost * rel_speed
            - c.pressure_cost * pressure,
    )
}

/// Время прибытия соперника `j` в точку.
fn opp_arrival(ctx: &Ctx, j: usize, p: Vec2) -> f32 {
    // Соперник, который уже в зоне досягаемости точки, реагировать не должен: он
    // перехватывает мяч мгновенно.
    let react = if ctx.opp_pos[j].distance(p) < ctx.cfg.control.reach_m {
        0.0
    } else {
        ctx.cfg.pass.defender_react_s
    };
    time_to_reach(ctx.opp_pos[j], ctx.opp_vel[j], &ctx.opp.body[j], react, p)
}

fn mate_arrival(ctx: &Ctx, j: usize, p: Vec2, react: f32) -> f32 {
    time_to_reach(
        ctx.mates_pos[j],
        ctx.mates_vel[j],
        &ctx.team.body[j],
        react,
        p,
    )
}

/// Контроль команды владельца в точке в момент `t_own`, когда туда придёт свой игрок.
fn control_at(ctx: &Ctx, p: Vec2, t_own: f32) -> f32 {
    let best_opp = (0..super::SLOTS)
        .map(|j| opp_arrival(ctx, j, p))
        .fold(f32::MAX, f32::min);
    sigmoid((best_opp - t_own) / ctx.cfg.pass.receiver_softness_s)
}

/// Цена потери мяча в точке: ценность позиции для соперника и число соперников впереди мяча.
fn loss_cost(ctx: &Ctx, p: Vec2) -> f32 {
    let ut = &ctx.cfg.utility;
    let ahead = ctx.opp_pos.iter().filter(|o| o.x < p.x).count() as f32;
    (ut.loss_base + ctx.xt.for_opponent(p)) * (1.0 + ut.ahead_gain * ahead)
}

/// Бонус стиля: короткие пасы, прямолинейность, длинные, ширина, темп.
fn style_bonus(ctx: &Ctx, c: &Candidate) -> f32 {
    let st = &ctx.team.style;
    let ut = &ctx.cfg.utility;
    let me = ctx.mates_pos[ctx.me];
    let forward = ((c.point.x - me.x) / 20.0).clamp(-1.0, 1.0);
    let lateral = ((c.point.y - me.y).abs() / 20.0).min(1.0);
    let dist = me.distance(c.point);
    match c.kind {
        Kind::PassFeet | Kind::PassSpace | Kind::Through | Kind::Long => {
            let short =
                f32::from(dist <= ctx.cfg.candidates.long_pass_min_m && !c.kind.is_lofted());
            let long = f32::from(c.kind.is_lofted() || c.kind == Kind::Through);
            ut.short_gain * (st.buildup_short - 0.5) * short
                + ut.direct_gain * (st.directness - 0.5) * forward
                + ut.long_gain * (st.directness - st.buildup_short) * long
                + ut.width_gain * (st.width_in - 0.5) * lateral
                + ut.tempo_gain * (st.tempo - 0.5) * forward
        }
        Kind::Dribble => {
            ut.dribble_gain * (ctx.team.mental[ctx.me].flair - 0.5)
                + ut.tempo_gain * (st.tempo - 0.5) * forward
        }
        _ => 0.0,
    }
}

/// Риск с учётом терпения: терпеливая команда сильнее боится потери.
fn risk_weight(ctx: &Ctx) -> f32 {
    let ut = &ctx.cfg.utility;
    ut.risk_weight * (1.0 + ut.patience_risk_gain * (ctx.team.style.patience - 0.5))
}

/// Точная оценка: заполняет `p`, `v`, `c`, `u`.
pub(super) fn evaluate(ctx: &Ctx, pressure: f32, c: &mut Candidate) {
    match c.kind {
        Kind::Shield => evaluate_shield(ctx, pressure, c),
        Kind::Dribble => evaluate_dribble(ctx, c),
        Kind::Shot => evaluate_shot(ctx, pressure, c),
        _ => evaluate_pass(ctx, pressure, c),
    }
}

/// Удар: полезность — вероятность гола с ценностью гола и небольшой ценой промаха.
fn evaluate_shot(ctx: &Ctx, pressure: f32, c: &mut Candidate) {
    let me = ctx.mates_pos[ctx.me];
    let sk = &ctx.team.skills[ctx.me];
    let sh = &ctx.cfg.shot;
    // Дальние удары (с 20 м) могут поощряться стилем; навык дальнего удара подменяет `finish`.
    let far = me.distance(c.point) >= LONG_SHOT_FROM_M;
    let skill = if far { sk.long_shot } else { sk.finish };
    c.p = super::shot::xg(ctx, me, pressure, skill);
    c.v = sh.goal_value;
    c.c = sh.miss_cost;
    let weight = ctx.team.style.final_third.long_shots - LONG_SHOT_BASE_WEIGHT;
    let bonus = if far { sh.long_shot_gain * weight } else { 0.0 };
    c.u = c.p * sh.goal_value - (1.0 - c.p) * sh.miss_cost + bonus;
}

fn evaluate_shield(ctx: &Ctx, pressure: f32, c: &mut Candidate) {
    let me = ctx.mates_pos[ctx.me];
    // Укрывание зависит от `shield` против давления; без давления оно просто удерживает мяч.
    let s = &ctx.team.skills[ctx.me];
    c.p = 1.0 - SHIELD_LOSS_AT_FULL_PRESSURE * pressure * (1.0 - s.shield);
    c.v = ctx.xt.at(me);
    c.c = loss_cost(ctx, me);
    c.u = c.p * c.v - (1.0 - c.p) * c.c * risk_weight(ctx)
        + ctx.cfg.utility.shield_value * pressure
        - ctx.cfg.utility.hold_cost_per_s * ctx.held_s;
}

fn evaluate_dribble(ctx: &Ctx, c: &mut Candidate) {
    let me = ctx.mates_pos[ctx.me];
    let (e, len) = (c.point, me.distance(c.point));
    let sk = &ctx.team.skills[ctx.me];
    let max_v = ctx.team.body[ctx.me].max_speed
        * (1.0
            - lerp(
                ctx.phys.player.ball_slowdown_max,
                ctx.phys.player.ball_slowdown_min,
                sk.dribble,
            ));
    let t_drib = len / max_v.max(1.0);
    // Соперник отбирает мяч, если успевает на путь и выигрывает единоборство.
    let mid = (me + e) * 0.5;
    let mut keep = 1.0f32;
    for j in 0..super::SLOTS {
        let t_j = opp_arrival(ctx, j, mid).min(opp_arrival(ctx, j, e));
        let reach = sigmoid((t_drib - t_j) / ctx.cfg.pass.intercept_softness_s);
        let duel = sigmoid(
            ctx.cfg.duel.skill_gain * (ctx.opp.skills[j].tackle - 0.5 * (sk.dribble + sk.shield)),
        );
        keep *= 1.0 - reach * duel;
    }
    c.p = keep;
    c.v = ctx.xt.at(e) * control_at(ctx, e, t_drib);
    c.c = loss_cost(ctx, mid);
    let carry = ctx.cfg.utility.carry_cost_m * len;
    c.u = c.p * c.v - (1.0 - c.p) * c.c * risk_weight(ctx) - carry + style_bonus(ctx, c)
        - ctx.cfg.utility.hold_cost_per_s * ctx.held_s;
}

/// Составляющие вероятности успеха паса (для отладки и калибровки).
#[derive(Clone, Copy, Debug, Default)]
pub struct PassTerms {
    pub clear: f32,
    pub timing: f32,
    pub reception: f32,
    pub accuracy: f32,
    pub t_end: f32,
    pub t_receiver: f32,
}

/// Разбор вероятности успеха паса по множителям.
pub fn pass_terms(ctx: &Ctx, pressure: f32, c: &Candidate) -> PassTerms {
    let pm = &ctx.cfg.pass;
    let me = ctx.mates_pos[ctx.me];
    let (target, s) = (c.point, me.distance(c.point));
    let launch = pass_launch(ctx, c.kind, me, target);
    let lofted = c.kind.is_lofted();

    // Перехват: для паса по земле на точках траектории, для верхового у точки приземления.
    let mut clear = 1.0f32;
    let (t_end, v_arrive);
    if lofted {
        t_end = launch.flight_time;
        v_arrive = launch.speed_h * LOFT_RECEIVE_SPEED_FACTOR;
        // Начало траектории мяч идёт низко: соперник рядом с пасующим успевает перехватить.
        let early = ball_flight(
            ctx.phys,
            launch.speed_h,
            LOFT_EARLY_LANE_M,
            LOFT_EARLY_POINTS,
        );
        let dir = (target - me) / s.max(1e-3);
        for j in 0..super::SLOTS {
            let t_j = opp_arrival(ctx, j, target);
            // Сигмоида монотонна: берём наибольший запас по времени и считаем её один раз.
            let mut margin = t_end - t_j;
            for k in 0..LOFT_EARLY_POINTS {
                let tk = early.times[k];
                if tk == f32::MAX {
                    break;
                }
                let p = me + dir * (LOFT_EARLY_LANE_M * (k + 1) as f32 / LOFT_EARLY_POINTS as f32);
                margin = margin.max(tk - opp_arrival(ctx, j, p));
            }
            clear *= 1.0 - sigmoid(margin / pm.intercept_softness_s);
        }
    } else {
        let n = pm.lane_points as usize;
        let prof = ball_flight(ctx.phys, launch.speed_h, s, n);
        t_end = if prof.t_end == f32::MAX {
            prof.times[n - 1].min(30.0)
        } else {
            prof.t_end
        };
        v_arrive = prof.v_arrive;
        let dir = (target - me) / s.max(1e-3);
        for j in 0..super::SLOTS {
            let mut margin = f32::MIN;
            for k in 0..n {
                let tk = prof.times[k];
                if tk == f32::MAX {
                    break;
                }
                let p = me + dir * (s * (k + 1) as f32 / n as f32);
                margin = margin.max(tk - opp_arrival(ctx, j, p));
            }
            if margin > f32::MIN {
                clear *= 1.0 - sigmoid(margin / pm.intercept_softness_s);
            }
        }
    }

    // Получатель: успеет ли к мячу, и сумеет ли принять.
    let recv = c.slot;
    let t_recv = if recv == NO_SLOT {
        0.0
    } else {
        mate_arrival(ctx, recv as usize, target, RECEIVER_REACT_S)
    };
    // Пас в ноги: получатель уже на месте и не должен бежать к мячу.
    let timing = if recv == NO_SLOT || c.kind == Kind::PassFeet {
        1.0
    } else {
        sigmoid((t_end - t_recv + RECEIVE_SLACK_S) / pm.receiver_softness_s)
    };
    let control_skill = if recv == NO_SLOT {
        0.0
    } else {
        ctx.team.skills[recv as usize].control - if lofted { LOFT_CONTROL_PENALTY } else { 0.0 }
    };
    let press_recv = ctx.pressure_at(target);
    let ctrl = if recv == NO_SLOT {
        1.0
    } else {
        reception_probability(ctx, control_skill, v_arrive, press_recv)
    };

    // Точность исполнения: промах поперёк и вдоль траектории против радиуса приёма.
    let skill = passer_skill(ctx, c.kind);
    let lateral = s * libm::sinf(angle_sigma(ctx, skill, pressure));
    let longitudinal = s * speed_sigma(ctx, skill, pressure) * 0.5;
    let erf = |reach: f32, sigma: f32| {
        if sigma < 1e-3 {
            1.0
        } else {
            libm::erff(reach / (core::f32::consts::SQRT_2 * sigma))
        }
    };
    let radius = pm.catch_radius_m * if lofted { LOFT_CATCH_FACTOR } else { 1.0 };
    let accuracy = erf(radius, lateral) * erf(radius * 1.5, longitudinal);

    PassTerms {
        clear,
        timing,
        reception: ctrl,
        accuracy,
        t_end,
        t_receiver: t_recv,
    }
}

fn evaluate_pass(ctx: &Ctx, pressure: f32, c: &mut Candidate) {
    let t = pass_terms(ctx, pressure, c);
    let scale = if c.kind.is_lofted() {
        ctx.cfg.pass.loft_success_scale
    } else {
        1.0
    };
    c.p = (t.clear * t.timing * t.reception * t.accuracy * scale).clamp(0.0, 1.0);
    let t_own = t.t_end.max(t.t_receiver);
    c.v = ctx.xt.at(c.point) * control_at(ctx, c.point, t_own);
    c.c = loss_cost(ctx, c.point);
    c.u = c.p * c.v - (1.0 - c.p) * c.c * risk_weight(ctx) + style_bonus(ctx, c);
    // Заведомо неточные пасы почти не выбираются; прямолинейные стили терпят больше риска.
    let ut = &ctx.cfg.utility;
    let p_min =
        ut.min_pass_success - ut.min_pass_success_directness * (ctx.team.style.directness - 0.5);
    if c.kind != Kind::Clearance && c.p < p_min {
        c.u -= ut.low_success_penalty;
    }
}
