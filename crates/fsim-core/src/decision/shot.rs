//! Удар по воротам: модель xG (`docs/03-decisions.md` §8) и оценка кандидата.

use super::Ctx;
use glam::Vec2;

/// Угол обзора ворот из точки (рад): ворота шириной `gw`, точка в системе команды.
pub fn goal_angle(from: Vec2, length: f32, gw: f32) -> f32 {
    let dx = (length - from.x).max(0.5);
    let dy = from.y;
    let a = libm::atan2f(gw * dx, dx * dx + dy * dy - (gw * 0.5) * (gw * 0.5));
    // Внутри створа (за линией штанг) арктангенс уходит в отрицательные значения.
    if a < 0.0 {
        a + core::f32::consts::PI
    } else {
        a
    }
}

/// Ожидаемые голы удара из точки `from` (система команды). Зависит от дистанции, угла обзора
/// ворот, давления и навыка завершения.
pub fn xg(ctx: &Ctx, from: Vec2, pressure: f32, finish: f32) -> f32 {
    let m = &ctx.cfg.shot.xg;
    let (len, gw) = (ctx.length(), ctx.phys.goal.width_m);
    let d = Vec2::new(len - from.x, from.y).length().max(m.min_dist_m);
    // Опорный угол: центр ворот с 11 метров.
    let reference = goal_angle(Vec2::new(len - 11.0, 0.0), len, gw);
    let af = (goal_angle(from, len, gw) / reference).max(0.05);
    let logit = m.a - m.b_ln_dist * libm::logf(d) + m.c_angle * libm::logf(af)
        - m.pressure * pressure
        + m.skill_gain * (finish - 0.5);
    1.0 / (1.0 + libm::expf(-logit))
}
