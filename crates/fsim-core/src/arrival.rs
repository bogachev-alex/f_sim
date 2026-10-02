//! Модель времени прибытия (подход Spearman и Fernández): за какое время игрок с текущей
//! скоростью, ускорением, максимальной скоростью и задержкой реакции достигает точки.
//! Главный примитив движка: перехват, контроль пространства, оценка кандидатов, прессинг.
//!
//! Модель: во время реакции игрок продолжает движение с текущей скоростью. Дальше движение
//! сводится к одной оси «на цель»: если игрок движется от цели, он сначала тормозит и
//! возвращается; затем разгоняется до максимальной скорости. Поперечная составляющая скорости
//! гасится боковым ускорением параллельно с разгоном и добавляет длину пути.

use crate::physics::body::BodyParams;
use crate::world::{Players, PLAYERS};
use glam::Vec2;

const EPS: f32 = 1e-4;
/// Доля бокового сноса, которая добавляется к длине пути: игрок гасит боковую скорость,
/// двигаясь по кривой погони, и путь длиннее прямой. Подобрано по интегрированию тела.
const DRIFT_PATH_FACTOR: f32 = 0.35;

/// Время (с) до точки `target` от состояния (`pos`, `vel`) при задержке реакции `react`.
#[inline]
pub fn time_to_reach(pos: Vec2, vel: Vec2, p: &BodyParams, react: f32, target: Vec2) -> f32 {
    // За время реакции игрок не меняет скорость.
    let p1 = pos + vel * react;
    let to = target - p1;
    let d = to.length();
    if d < EPS {
        return react;
    }
    let dir = to / d;
    let u = vel.dot(dir);
    let perp = vel - dir * u;
    let w = perp.length();

    // Радиальный участок: возврат к нулевой скорости, если движемся от цели.
    let (t0, s0, dist) = if u < 0.0 {
        (-u / p.decel, 0.0, d + u * u / (2.0 * p.decel))
    } else {
        (0.0, u.min(p.max_speed), d)
    };
    // Поперечная скорость гасится за w / lat_accel; за это время игрок смещается вбок на
    // w² / (2 lat_accel), что удлиняет путь.
    let drift = w * w / (2.0 * p.lat_accel);
    let dist = dist + DRIFT_PATH_FACTOR * drift;

    let t_acc = (p.max_speed - s0) / p.accel;
    let d_acc = (p.max_speed * p.max_speed - s0 * s0) / (2.0 * p.accel);
    let t = if dist <= d_acc {
        (libm::sqrtf(s0 * s0 + 2.0 * p.accel * dist) - s0) / p.accel
    } else {
        t_acc + (dist - d_acc) / p.max_speed
    };
    react + t0 + t
}

/// Времена прибытия всех игроков к одной точке. Цикл по полям SoA без обращений к соседям,
/// чтобы компилятор мог его векторизовать.
pub fn arrival_all(
    players: &Players,
    body: &[BodyParams; PLAYERS],
    react: &[f32; PLAYERS],
    target: Vec2,
    out: &mut [f32; PLAYERS],
) {
    for i in 0..PLAYERS {
        out[i] = time_to_reach(
            Vec2::new(players.pos_x[i], players.pos_y[i]),
            Vec2::new(players.vel_x[i], players.vel_y[i]),
            &body[i],
            react[i],
            target,
        );
    }
}
