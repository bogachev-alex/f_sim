//! Контроль пространства: чья команда первой достигает точки (модель прибытия + логистика).
//! Сетка 21×14 раз в 0,5 с и точный расчёт в одной точке для кандидатов
//! (`docs/02-engine-core.md` §6).

use crate::arrival::time_to_reach;
use crate::physics::body::BodyParams;
use crate::world::{Players, PLAYERS};
use glam::Vec2;

pub const GRID_COLS: usize = 21;
pub const GRID_ROWS: usize = 14;

/// Вероятность, что команда 0 контролирует точку, при разности времён прибытия `dt` (с):
/// `dt = T_команды1 − T_команды0`.
#[inline]
pub fn control_probability(dt: f32, softness_s: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-dt / softness_s))
}

/// Время прибытия самого быстрого игрока каждой команды в точку (мировые координаты).
pub fn fastest_arrival(
    players: &Players,
    body: &[BodyParams; PLAYERS],
    react: f32,
    target: Vec2,
) -> [f32; 2] {
    let mut best = [f32::MAX; 2];
    for (i, b) in body.iter().enumerate() {
        let t = time_to_reach(
            Vec2::new(players.pos_x[i], players.pos_y[i]),
            Vec2::new(players.vel_x[i], players.vel_y[i]),
            b,
            react,
            target,
        );
        let k = usize::from(i >= PLAYERS / 2);
        if t < best[k] {
            best[k] = t;
        }
    }
    best
}

/// Контроль команды `team` в точке: вероятность, что она достигнет точки первой.
pub fn control_at(
    players: &Players,
    body: &[BodyParams; PLAYERS],
    react: f32,
    softness_s: f32,
    point: Vec2,
    team: usize,
) -> f32 {
    let t = fastest_arrival(players, body, react, point);
    let (own, other) = (t[team], t[1 - team]);
    control_probability(other - own, softness_s)
}

/// Сетка контроля команды 0 по всему полю.
#[derive(Clone)]
pub struct ControlGrid {
    pub cells: [[f32; GRID_COLS]; GRID_ROWS],
    length: f32,
    width: f32,
}

impl ControlGrid {
    pub fn new(length: f32, width: f32) -> ControlGrid {
        ControlGrid {
            cells: [[0.5; GRID_COLS]; GRID_ROWS],
            length,
            width,
        }
    }

    pub fn cell_center(&self, row: usize, col: usize) -> Vec2 {
        Vec2::new(
            (col as f32 + 0.5) / GRID_COLS as f32 * self.length - self.length * 0.5,
            (row as f32 + 0.5) / GRID_ROWS as f32 * self.width - self.width * 0.5,
        )
    }

    /// Пересчёт: для каждой клетки времена прибытия самых быстрых игроков каждой команды.
    pub fn update(
        &mut self,
        players: &Players,
        body: &[BodyParams; PLAYERS],
        react: f32,
        softness_s: f32,
    ) {
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                self.cells[row][col] = control_at(
                    players,
                    body,
                    react,
                    softness_s,
                    self.cell_center(row, col),
                    0,
                );
            }
        }
    }

    /// Контроль команды 0 в точке по ближайшей клетке (билинейная интерполяция).
    pub fn sample(&self, p: Vec2) -> f32 {
        let fx = ((p.x + self.length * 0.5) / self.length * GRID_COLS as f32 - 0.5)
            .clamp(0.0, (GRID_COLS - 1) as f32);
        let fy = ((p.y + self.width * 0.5) / self.width * GRID_ROWS as f32 - 0.5)
            .clamp(0.0, (GRID_ROWS - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(GRID_COLS - 1), (y0 + 1).min(GRID_ROWS - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let c = &self.cells;
        let top = c[y0][x0] * (1.0 - tx) + c[y0][x1] * tx;
        let bottom = c[y1][x0] * (1.0 - tx) + c[y1][x1] * tx;
        top * (1.0 - ty) + bottom * ty
    }
}
