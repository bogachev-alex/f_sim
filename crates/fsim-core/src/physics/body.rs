//! Кинематика игрока. Ускорение ограничено по трём осям: вдоль скорости (разгон), против
//! (торможение) и поперёк (боковое). Радиус разворота получается сам: r = v² / a_lat,
//! растёт со скоростью и падает с ростом `agility`.

use crate::attrs::{Attr, Attrs};
use crate::config::PhysicsConfig;
use crate::curves::CurveSet;
use glam::Vec2;

/// Минимальная длина вектора, ниже которой направление считается неопределённым.
const EPS: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyParams {
    pub max_speed: f32,
    pub accel: f32,
    pub decel: f32,
    pub lat_accel: f32,
}

impl BodyParams {
    /// Параметры из атрибутов `pace`, `acceleration`, `agility` через кривые отклика.
    pub fn from_attrs(attrs: &Attrs, curves: &CurveSet, phys: &PhysicsConfig) -> BodyParams {
        let p = &phys.player;
        let lerp = |lo: f32, hi: f32, a: Attr| lo + (hi - lo) * curves.eval(a, attrs[a as usize]);
        BodyParams {
            max_speed: lerp(p.max_speed_min_ms, p.max_speed_max_ms, Attr::pace),
            accel: lerp(
                p.acceleration_min_ms2,
                p.acceleration_max_ms2,
                Attr::acceleration,
            ),
            decel: p.deceleration_ms2,
            lat_accel: lerp(
                p.lateral_accel_min_ms2,
                p.lateral_accel_max_ms2,
                Attr::agility,
            ),
        }
    }
}

/// Режим интенсивности: доля от максимальной скорости.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Gait {
    Walk,
    Jog,
    Sprint,
}

impl Gait {
    pub fn fraction(self, phys: &PhysicsConfig) -> f32 {
        match self {
            Gait::Walk => phys.player.gait_walk_fraction,
            Gait::Jog => phys.player.gait_jog_fraction,
            Gait::Sprint => 1.0,
        }
    }
}

/// Шаг тела к цели.
///
/// `speed_cap`: желаемая скорость, не выше `max_speed`. `arrive`: тормозить, чтобы остановиться
/// в цели; иначе идти сквозь цель на `speed_cap`.
#[inline]
pub fn step_body(
    pos: &mut Vec2,
    vel: &mut Vec2,
    p: &BodyParams,
    target: Vec2,
    speed_cap: f32,
    arrive: bool,
    dt: f32,
) {
    let to = target - *pos;
    let dist = to.length();
    let cap = speed_cap.min(p.max_speed);
    if dist < EPS {
        // В самой цели оси нет: гасим скорость вдоль неё самой.
        let speed = vel.length();
        let axis = if speed > EPS { *vel / speed } else { Vec2::X };
        apply_desired_speed(vel, p, axis, 0.0, dt);
    } else {
        let axis = to / dist;
        let mut s = cap;
        if arrive {
            // Скорость, с которой ещё можно остановиться в цели, и не перелететь её за шаг.
            s = s.min(libm::sqrtf(2.0 * p.decel * dist)).min(dist / dt);
        }
        apply_desired_speed(vel, p, axis, s, dt);
    }
    *pos += *vel * dt;
}

/// Приближение скорости к желаемой `axis * desired_speed`.
///
/// Скорость раскладывается по оси на цель и поперёк неё, лимиты независимы:
/// - вдоль оси: разгон к цели `accel`, торможение `decel`; движение от цели гасится
///   торможением, а затем начинается разгон;
/// - поперёк: боковое ускорение `lat_accel` гасит боковую скорость.
///
/// Радиус разворота получается сам: r = v² / lat_accel. Ось задана целью, а не скоростью,
/// поэтому на малых скоростях нет вырождения направления и дрожания.
#[inline]
pub fn apply_desired_speed(vel: &mut Vec2, p: &BodyParams, axis: Vec2, desired: f32, dt: f32) {
    let vr = vel.dot(axis);
    let lateral = *vel - axis * vr;
    let dv_r = desired - vr;
    let step_r = if dv_r < 0.0 || vr < 0.0 {
        dv_r.clamp(-p.decel * dt, p.decel * dt)
    } else {
        dv_r.min(p.accel * dt)
    };
    let lat_len = lateral.length();
    let lat_step = p.lat_accel * dt;
    let lateral_new = if lat_len > lat_step {
        lateral * ((lat_len - lat_step) / lat_len)
    } else {
        Vec2::ZERO
    };
    *vel = axis * (vr + step_r) + lateral_new;
    // Сумма независимых составляющих не должна давать скорость выше предела.
    let sp = vel.length();
    if sp > p.max_speed {
        *vel *= p.max_speed / sp;
    }
}
