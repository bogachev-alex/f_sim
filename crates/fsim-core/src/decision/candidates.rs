//! Кандидаты владельца мяча и быстрая оценка этапа 1 (`docs/03-decisions.md` §1, §3).

use super::{Candidate, Ctx, Kind, MAX_CANDIDATES};
use crate::positioning::team::Group;
use glam::Vec2;

/// Расстояние от точки до отрезка.
#[inline]
pub(super) fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Ближайший к линии паса соперник: расстояние до отрезка.
fn lane_gap(ctx: &Ctx, a: Vec2, b: Vec2) -> f32 {
    ctx.opp_pos
        .iter()
        .map(|&o| dist_to_segment(o, a, b))
        .fold(f32::MAX, f32::min)
}

fn nearest_opp(ctx: &Ctx, p: Vec2) -> f32 {
    ctx.opp_pos
        .iter()
        .map(|&o| o.distance(p))
        .fold(f32::MAX, f32::min)
}

/// Линия защитников соперника: `X` предпоследнего по глубине соперника.
pub(super) fn opp_line_x(ctx: &Ctx) -> f32 {
    let (mut hi, mut second) = (f32::MIN, f32::MIN);
    for o in ctx.opp_pos {
        if o.x > hi {
            second = hi;
            hi = o.x;
        } else if o.x > second {
            second = o.x;
        }
    }
    second
}

pub(super) fn clamp_to_pitch(ctx: &Ctx, p: Vec2) -> Vec2 {
    let margin = 1.5;
    Vec2::new(
        p.x.clamp(margin, ctx.length() - margin),
        p.y.clamp(-ctx.width() * 0.5 + margin, ctx.width() * 0.5 - margin),
    )
}

/// Строит кандидатов и считает оценку этапа 1. Возвращает их число.
pub(super) fn generate(ctx: &Ctx, pressure: f32, out: &mut [Candidate; MAX_CANDIDATES]) -> usize {
    let cfg = &ctx.cfg.candidates;
    let ut = &ctx.cfg.utility;
    let me = ctx.mates_pos[ctx.me];
    let xt_me = ctx.xt.at(me);
    let (len, wid) = (ctx.length(), ctx.width());
    let goal = Vec2::new(len, 0.0);
    let line_x = opp_line_x(ctx);
    let mut n = 0usize;
    let mut push = |c: Candidate| {
        if n < MAX_CANDIDATES {
            out[n] = c;
            n += 1;
        }
    };
    let pass = |kind: Kind, slot: usize, point: Vec2, h: f32| Candidate {
        kind,
        slot: slot as u8,
        point,
        h,
        ..Candidate::EMPTY
    };

    for j in 0..super::SLOTS {
        if j == ctx.me {
            continue;
        }
        let mate = ctx.mates_pos[j];
        let dist = me.distance(mate);
        if dist < 2.5 {
            continue;
        }
        let keeper = ctx.team.slots[j].group == Group::Keeper;
        // Пас в ноги: на упреждение по скорости партнёра.
        let feet = clamp_to_pitch(ctx, mate + ctx.mates_vel[j] * 0.4);
        let c = (lane_gap(ctx, me, feet) / ut.lane_clear_ref_m).clamp(0.0, 1.0);
        push(pass(
            Kind::PassFeet,
            j,
            feet,
            c * (ctx.xt.at(feet) + 0.001) - xt_me,
        ));
        // Длинный верховой пас: контроль определяется у точки приземления.
        if dist >= cfg.long_pass_min_m {
            let c = (nearest_opp(ctx, feet) / ut.lane_clear_ref_m).clamp(0.0, 1.0) * 0.8 + 0.2;
            push(pass(
                Kind::Long,
                j,
                feet,
                c * (ctx.xt.at(feet) + 0.001) - xt_me,
            ));
        }
        if keeper {
            continue;
        }
        // Пас в пространство перед партнёром: вперёд и немного к центру.
        let dir = Vec2::new(1.0, -0.3 * mate.y.signum()).normalize();
        for k in 1..=cfg.space_points {
            let p = clamp_to_pitch(ctx, mate + dir * (cfg.space_step_m * k as f32));
            if me.distance(p) > 55.0 {
                continue;
            }
            let c = (lane_gap(ctx, me, p) / ut.lane_clear_ref_m).clamp(0.0, 1.0);
            push(pass(
                Kind::PassSpace,
                j,
                p,
                c * (ctx.xt.at(p) + 0.001) - xt_me,
            ));
        }
        // Пас за спину защитникам: партнёр у линии защиты, мяч глубже неё.
        if mate.x >= line_x - 8.0 && mate.x > me.x + 5.0 {
            let p = clamp_to_pitch(ctx, Vec2::new(line_x + cfg.through_depth_m, mate.y));
            let c = (lane_gap(ctx, me, p) / ut.lane_clear_ref_m).clamp(0.0, 1.0);
            push(pass(
                Kind::Through,
                j,
                p,
                c * (ctx.xt.at(p) + 0.001) - xt_me,
            ));
        }
    }

    // Ведение в нескольких направлениях веером от направления на чужие ворота.
    let to_goal = (goal - me).normalize_or_zero();
    let base = libm::atan2f(to_goal.y, to_goal.x);
    let spread = cfg.dribble_spread_deg.to_radians();
    let n_dirs = cfg.dribble_dirs.max(1);
    let length = 0.5 * (cfg.dribble_min_m + cfg.dribble_max_m);
    for d in 0..n_dirs {
        let off = if n_dirs > 1 {
            (d as f32 / (n_dirs - 1) as f32 - 0.5) * 2.0 * spread
        } else {
            0.0
        };
        let dir = Vec2::new(libm::cosf(base + off), libm::sinf(base + off));
        let e = clamp_to_pitch(ctx, me + dir * length);
        let safe = (nearest_opp(ctx, e) / 6.0).clamp(0.0, 1.0);
        let h = safe * ctx.xt.at(e) - xt_me - ut.carry_cost_m * length;
        push(Candidate {
            kind: Kind::Dribble,
            dir: d as u8,
            point: e,
            h,
            ..Candidate::EMPTY
        });
    }
    // Укрывание.
    push(Candidate {
        kind: Kind::Shield,
        point: me,
        h: -0.3 * xt_me + ut.shield_value * pressure,
        ..Candidate::EMPTY
    });
    // Удар: из пределов дальности по воротам.
    let to_goal_dist = Vec2::new(len - me.x, me.y).length();
    if to_goal_dist <= ctx.cfg.shot.max_distance_m && me.x < len - 1.0 {
        let finish = ctx.team.skills[ctx.me]
            .finish
            .max(ctx.team.skills[ctx.me].long_shot * 0.5);
        let g = ctx.cfg.shot.goal_value;
        let x = super::shot::xg(ctx, me, pressure, finish);
        push(Candidate {
            kind: Kind::Shot,
            point: Vec2::new(len, 0.0),
            h: x * g - xt_me,
            ..Candidate::EMPTY
        });
    }
    // Вынос у своих ворот под давлением: на фланг, где меньше соперников.
    if me.x <= cfg.clearance_zone_m {
        let side = if ctx.opp_pos.iter().map(|o| o.y).sum::<f32>() >= 0.0 {
            -1.0
        } else {
            1.0
        };
        let p = clamp_to_pitch(
            ctx,
            Vec2::new(
                (me.x + cfg.clearance_distance_m).min(len - 10.0),
                side * (wid * 0.5 - 8.0),
            ),
        );
        let bias = if pressure > 0.5 { 0.02 } else { -0.01 };
        push(Candidate {
            kind: Kind::Clearance,
            point: p,
            h: ctx.xt.at(p) * 0.6 - xt_me + bias,
            ..Candidate::EMPTY
        });
    }
    n
}
