//! Метрики формы команды по фактическим позициям игроков (`docs/07-validation.md` §3, §9).

use crate::positioning::team::{Group, TeamSetup};
use crate::positioning::world_to_team;
use crate::world::{World, PLAYERS};
use glam::Vec2;

const SLOTS: usize = 11;

/// Форма команды в системе команды (метры от своей линии ворот).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapeMetrics {
    /// Средняя `X` защитников (линия обороны).
    pub back_x: f32,
    /// `X` самого переднего полевого игрока.
    pub front_x: f32,
    /// Расстояние от линии защитников до переднего игрока.
    pub depth: f32,
    /// Длина блока: от самого заднего до самого переднего полевого.
    pub length: f32,
    /// Ширина блока по полевым игрокам.
    pub width: f32,
    /// Средняя `X` полевых игроков.
    pub mean_x: f32,
}

pub fn shape_metrics(world: &World, team: &TeamSetup, k: usize, half_len: f32) -> ShapeMetrics {
    let p = &world.players;
    let (mut back_sum, mut back_n) = (0.0f32, 0u32);
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    let mut sum_x = 0.0f32;
    let mut n = 0u32;
    for i in 0..SLOTS {
        let s = &team.slots[i];
        if s.group == Group::Keeper {
            continue;
        }
        let idx = k * SLOTS + i;
        let t = world_to_team(k, Vec2::new(p.pos_x[idx], p.pos_y[idx]), half_len);
        min_x = min_x.min(t.x);
        max_x = max_x.max(t.x);
        min_y = min_y.min(t.y);
        max_y = max_y.max(t.y);
        sum_x += t.x;
        n += 1;
        if s.group == Group::Back {
            back_sum += t.x;
            back_n += 1;
        }
    }
    let back_x = back_sum / back_n.max(1) as f32;
    ShapeMetrics {
        back_x,
        front_x: max_x,
        depth: max_x - back_x,
        length: max_x - min_x,
        width: max_y - min_y,
        mean_x: sum_x / n.max(1) as f32,
    }
}

/// Считает смены направления: пара отсчётов скорости, угол между которыми больше `min_angle`,
/// при скорости выше `min_speed` в обоих. Это основа детектора дрожания.
#[derive(Clone, Debug)]
pub struct ReversalCounter {
    prev: [Vec2; PLAYERS],
    pub reversals: u32,
    pub samples: u32,
}

impl Default for ReversalCounter {
    fn default() -> Self {
        ReversalCounter {
            prev: [Vec2::ZERO; PLAYERS],
            reversals: 0,
            samples: 0,
        }
    }
}

impl ReversalCounter {
    pub fn sample(&mut self, world: &World, min_speed: f32, min_angle: f32) {
        let p = &world.players;
        let cos_limit = libm::cosf(min_angle);
        for i in 0..PLAYERS {
            let v = Vec2::new(p.vel_x[i], p.vel_y[i]);
            let (a, b) = (self.prev[i].length(), v.length());
            if a > min_speed && b > min_speed {
                self.samples += 1;
                if self.prev[i].dot(v) / (a * b) < cos_limit {
                    self.reversals += 1;
                }
            }
            self.prev[i] = v;
        }
    }
}

/// Сколько полевых игроков (обеих команд) ближе `radius` к мячу: основа детектора роя
/// (`docs/07-validation.md` §9). Вратари не считаются.
pub fn players_near_ball(world: &World, radius: f32) -> usize {
    let p = &world.players;
    let r2 = radius * radius;
    let mut n = 0;
    for i in 0..PLAYERS {
        if i % SLOTS == 0 {
            continue;
        }
        let (dx, dy) = (p.pos_x[i] - world.ball_pos.x, p.pos_y[i] - world.ball_pos.y);
        if dx * dx + dy * dy < r2 {
            n += 1;
        }
    }
    n
}
