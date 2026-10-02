//! 3D-мяч: гравитация, квадратичное сопротивление воздуха относительно ветра, отскок от
//! земли, качение с трением, штанги и перекладина (цилиндры) и сетка (двусторонние
//! прямоугольники). Движение по подшагам с непрерывной проверкой столкновений отрезком:
//! быстрый мяч не проходит сквозь стойку.
//!
//! Система координат: центр поля в нуле, `x` вдоль поля, `y` поперёк, `z` вверх.
//! Ворота стоят на линиях `x = ±length/2`.

use crate::config::PhysicsConfig;
use glam::{Vec2, Vec3};

/// Сдвиг точки после столкновения вдоль нормали, чтобы не застревать в поверхности.
const SKIN: f32 = 1e-4;
/// Предел числа столкновений за подшаг.
const MAX_HITS: usize = 4;

/// Внешние условия: ветер, дождь, качество поля (отскок 0..1 между границами из конфига).
#[derive(Clone, Copy, Debug)]
pub struct BallEnv {
    pub wind: Vec2,
    pub rain: f32,
    pub pitch_bounce: f32,
}

impl Default for BallEnv {
    fn default() -> Self {
        BallEnv {
            wind: Vec2::ZERO,
            rain: 0.0,
            pitch_bounce: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BallModel {
    r: f32,
    g: f32,
    drag: f32,
    roll: f32,
    e_min: f32,
    e_max: f32,
    tang_keep: f32,
    min_vz: f32,
    rain_bounce: f32,
    rain_roll: f32,
    substeps: u32,
    half_len: f32,
    half_goal: f32,
    goal_h: f32,
    goal_d: f32,
    post_r: f32,
    post_e: f32,
    net_e: f32,
    net_keep: f32,
}

struct Hit {
    t: f32,
    normal: Vec3,
    restitution: f32,
    tang_keep: f32,
}

impl BallModel {
    pub fn new(c: &PhysicsConfig) -> BallModel {
        BallModel {
            r: c.ball.radius_m,
            g: c.ball.gravity_ms2,
            drag: c.ball.air_drag_per_m,
            roll: c.ball.rolling_decel_ms2,
            e_min: c.ball.bounce_restitution_min,
            e_max: c.ball.bounce_restitution_max,
            tang_keep: c.ball.bounce_tangential_keep,
            min_vz: c.ball.bounce_min_vz_ms,
            rain_bounce: c.ball.rain_bounce_scale,
            rain_roll: c.ball.rain_rolling_scale,
            substeps: c.ball.substeps,
            half_len: c.pitch.length_m * 0.5,
            half_goal: c.goal.width_m * 0.5,
            goal_h: c.goal.height_m,
            goal_d: c.goal.depth_m,
            post_r: c.goal.post_radius_m,
            post_e: c.goal.post_restitution,
            net_e: c.goal.net_restitution,
            net_keep: c.goal.net_tangential_keep,
        }
    }

    pub fn radius(&self) -> f32 {
        self.r
    }

    /// Один физический шаг `dt`, разбитый на подшаги.
    pub fn step(&self, pos: &mut Vec3, vel: &mut Vec3, env: &BallEnv, dt: f32) {
        let h = dt / self.substeps as f32;
        let wind = Vec3::new(env.wind.x, env.wind.y, 0.0);
        let e = (self.e_min + (self.e_max - self.e_min) * env.pitch_bounce)
            * (1.0 - env.rain * (1.0 - self.rain_bounce));
        let roll = self.roll * (1.0 - env.rain * (1.0 - self.rain_roll));
        for _ in 0..self.substeps {
            let rel = *vel - wind;
            let acc = -rel * (self.drag * rel.length()) + Vec3::new(0.0, 0.0, -self.g);
            *vel += acc * h;
            self.advance(pos, vel, h);
            self.depenetrate(pos, vel);
            self.ground(pos, vel, e);
            if pos.z <= self.r + SKIN && vel.z == 0.0 {
                // Качение: горизонтальная скорость падает на постоянное замедление.
                let sp = Vec2::new(vel.x, vel.y).length();
                if sp > 0.0 {
                    let k = (sp - roll * h).max(0.0) / sp;
                    vel.x *= k;
                    vel.y *= k;
                }
            }
        }
    }

    /// Перемещение на время `h` с обработкой столкновений с воротами.
    fn advance(&self, pos: &mut Vec3, vel: &mut Vec3, h: f32) {
        let mut remaining = 1.0f32;
        for _ in 0..MAX_HITS {
            let new = *pos + *vel * (h * remaining);
            match self.collide(*pos, new) {
                Some(hit) => {
                    *pos += (new - *pos) * hit.t + hit.normal * SKIN;
                    let vn = vel.dot(hit.normal);
                    if vn < 0.0 {
                        let v_n = hit.normal * vn;
                        *vel = (*vel - v_n) * hit.tang_keep - v_n * hit.restitution;
                    }
                    remaining *= 1.0 - hit.t;
                    if remaining < 1e-3 {
                        return;
                    }
                }
                None => {
                    *pos = new;
                    return;
                }
            }
        }
    }

    /// Мяч, оказавшийся внутри штанги или перекладины (например, скатился на них по сетке),
    /// выталкивается на поверхность; скорость внутрь отражается.
    fn depenetrate(&self, pos: &mut Vec3, vel: &mut Vec3) {
        let rr = self.post_r + self.r;
        let (hl, hw, gh) = (self.half_len, self.half_goal, self.goal_h);
        if pos.x.abs() < hl - rr {
            return;
        }
        let s = if pos.x >= 0.0 { 1.0f32 } else { -1.0 };
        let (x, y0, z0) = (pos.x * s, pos.y, pos.z);
        // Центры трубок в локальной системе (ворота на +x): штанги и перекладина.
        let mut push = |cx: f32, cy: f32, cz: f32, axis_y: bool| {
            // Для штанги ось вертикальна (плоскость xy), для перекладины вдоль y (плоскость xz).
            let (du, dv) = if axis_y {
                (x - cx, pos.z - cz)
            } else {
                (x - cx, pos.y - cy)
            };
            let d = libm::sqrtf(du * du + dv * dv);
            if d >= rr || d < 1e-6 {
                return;
            }
            let (nu, nv) = (du / d, dv / d);
            let k = rr + SKIN - d;
            let n = if axis_y {
                Vec3::new(nu * s, 0.0, nv)
            } else {
                Vec3::new(nu * s, nv, 0.0)
            };
            let shift = if axis_y {
                Vec3::new(nu * k * s, 0.0, nv * k)
            } else {
                Vec3::new(nu * k * s, nv * k, 0.0)
            };
            *pos += shift;
            let vn = vel.dot(n);
            if vn < 0.0 {
                *vel -= n * (vn * (1.0 + self.post_e));
            }
        };
        if y0.abs() <= hw {
            push(hl, 0.0, gh, true);
        }
        if z0 <= gh {
            for sy in [hw, -hw] {
                push(hl, sy, 0.0, false);
            }
        }
    }

    fn ground(&self, pos: &mut Vec3, vel: &mut Vec3, e: f32) {
        if pos.z < self.r {
            pos.z = self.r;
            if vel.z < 0.0 {
                if -vel.z > self.min_vz {
                    vel.z = -vel.z * e;
                    vel.x *= self.tang_keep;
                    vel.y *= self.tang_keep;
                } else {
                    vel.z = 0.0;
                }
            }
        }
    }

    /// Самое раннее столкновение отрезка `p0 → p1` с обоими воротами.
    fn collide(&self, p0: Vec3, p1: Vec3) -> Option<Hit> {
        // Быстрый отказ: ворота и штанги только у торцов поля.
        let reach = self.half_len - self.r - self.post_r - SKIN;
        if p0.x.abs().max(p1.x.abs()) < reach {
            return None;
        }
        let mut best: Option<Hit> = None;
        for s in [1.0f32, -1.0] {
            // Локальная система: ворота на +x. Отражение по x меняет знак нормали обратно.
            let a = Vec3::new(p0.x * s, p0.y, p0.z);
            let b = Vec3::new(p1.x * s, p1.y, p1.z);
            if let Some(mut hit) = self.collide_goal(a, b) {
                hit.normal.x *= s;
                if best.as_ref().is_none_or(|bh| hit.t < bh.t) {
                    best = Some(hit);
                }
            }
        }
        best
    }

    fn collide_goal(&self, a: Vec3, b: Vec3) -> Option<Hit> {
        let d = b - a;
        let hl = self.half_len;
        let (hw, gh, gd) = (self.half_goal, self.goal_h, self.goal_d);
        let rr = self.post_r + self.r;
        let mut best: Option<Hit> = None;
        let mut consider = |h: Hit| {
            if best.as_ref().is_none_or(|bh| h.t < bh.t) {
                best = Some(h);
            }
        };

        // Штанги: вертикальные цилиндры до высоты перекладины.
        for sy in [1.0f32, -1.0] {
            let c = Vec2::new(hl, sy * hw);
            if let Some(t) = seg_circle(Vec2::new(a.x, a.y), Vec2::new(d.x, d.y), c, rr) {
                let q = a + d * t;
                if q.z <= gh {
                    let n = Vec2::new(q.x - c.x, q.y - c.y).normalize_or_zero();
                    consider(Hit {
                        t,
                        normal: Vec3::new(n.x, n.y, 0.0),
                        restitution: self.post_e,
                        tang_keep: 1.0,
                    });
                }
            }
        }
        // Перекладина: цилиндр вдоль `y` на высоте `gh`.
        if let Some(t) = seg_circle(
            Vec2::new(a.x, a.z),
            Vec2::new(d.x, d.z),
            Vec2::new(hl, gh),
            rr,
        ) {
            let q = a + d * t;
            if q.y.abs() <= hw {
                let n = Vec2::new(q.x - hl, q.z - gh).normalize_or_zero();
                consider(Hit {
                    t,
                    normal: Vec3::new(n.x, 0.0, n.y),
                    restitution: self.post_e,
                    tang_keep: 1.0,
                });
            }
        }
        // Сетка: задняя стенка, две боковые, крыша. Прямоугольники двусторонние.
        let r = self.r;
        let mut plane = |axis: usize, c: f32, lo: [f32; 3], hi: [f32; 3]| {
            let s0 = a[axis] - c;
            let side = if s0 >= 0.0 { 1.0 } else { -1.0 };
            let in_band = s0.abs() < r;
            if in_band {
                // Мяч уже вплотную к сетке (например, протиснулся между штангой и боковой
                // сеткой): отражаем сразу, если он движется к ней, иначе пропускаем.
                if (b[axis] - a[axis]) * side >= 0.0 {
                    return;
                }
            } else if (b[axis] - c) * side >= r {
                return; // не дошёл до поверхности
            }
            let t = if in_band {
                0.0
            } else {
                (a[axis] - (c + side * r)) / (a[axis] - b[axis])
            };
            let q = a + d * t;
            for k in 0..3 {
                if k != axis && (q[k] < lo[k] - r || q[k] > hi[k] + r) {
                    return;
                }
            }
            let mut n = Vec3::ZERO;
            n[axis] = side;
            consider(Hit {
                t,
                normal: n,
                restitution: self.net_e,
                tang_keep: self.net_keep,
            });
        };
        let lo = [hl, -hw, 0.0];
        let hi = [hl + gd, hw, gh];
        plane(0, hl + gd, lo, hi); // задняя
        plane(1, hw, lo, hi); // боковые
        plane(1, -hw, lo, hi);
        plane(2, gh, lo, hi); // крыша
        best
    }
}

/// Первое пересечение отрезка `p + t d` (t в [0, 1]) с окружностью, снаружи внутрь.
fn seg_circle(p: Vec2, d: Vec2, c: Vec2, radius: f32) -> Option<f32> {
    let f = p - c;
    let a = d.dot(d);
    if a < 1e-12 {
        return None;
    }
    let cc = f.dot(f) - radius * radius;
    if cc <= 0.0 {
        return None; // уже внутри
    }
    let b = 2.0 * f.dot(d);
    let disc = b * b - 4.0 * a * cc;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - libm::sqrtf(disc)) / (2.0 * a);
    (0.0..=1.0).contains(&t).then_some(t)
}
