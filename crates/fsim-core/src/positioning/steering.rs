//! Движение к якорю: желаемая скорость по срочности и `work_rate`.

use super::lerp;
use crate::config::Steering;

/// Желаемая скорость (м/с): шаг у якоря, бег на максимуме далеко от него.
/// Ближе `near_m` игрок идёт шагом, дальше `far_m` бежит на максимальной скорости;
/// высокий `work_rate` сокращает `far_m`, и игрок раньше переходит на бег.
#[inline]
pub fn speed_cap(
    dist: f32,
    max_speed: f32,
    walk_fraction: f32,
    work_rate: f32,
    cfg: &Steering,
) -> f32 {
    let far = cfg.far_m * (1.0 - cfg.work_rate_far_shrink * work_rate);
    let t = ((dist - cfg.near_m) / (far - cfg.near_m).max(1e-3)).clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    max_speed * lerp(walk_fraction, 1.0, smooth)
}
