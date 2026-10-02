//! Ценность позиции: xT клетки и контроль пространства (`docs/02-engine-core.md` §6).

use crate::config::XtGrid;
use glam::Vec2;

/// Сетка xT с билинейной интерполяцией. Координаты в системе команды: `X` от своих ворот.
#[derive(Clone, Debug)]
pub struct Xt {
    grid: XtGrid,
    length: f32,
    width: f32,
}

impl Xt {
    pub fn new(grid: XtGrid, length: f32, width: f32) -> Xt {
        Xt {
            grid,
            length,
            width,
        }
    }

    /// Ценность точки для команды, атакующей в +X.
    pub fn at(&self, p: Vec2) -> f32 {
        let g = &self.grid;
        // Центры клеток: смещение на полклетки.
        let fx = (p.x / self.length * g.cols as f32 - 0.5).clamp(0.0, (g.cols - 1) as f32);
        let fy = ((p.y + self.width * 0.5) / self.width * g.rows as f32 - 0.5)
            .clamp(0.0, (g.rows - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(g.cols - 1), (y0 + 1).min(g.rows - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let v = |x: usize, y: usize| g.values[y * g.cols + x];
        let top = v(x0, y0) * (1.0 - tx) + v(x1, y0) * tx;
        let bottom = v(x0, y1) * (1.0 - tx) + v(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    /// Ценность той же точки для соперника, который атакует в обратную сторону.
    pub fn for_opponent(&self, p: Vec2) -> f32 {
        self.at(Vec2::new(self.length - p.x, -p.y))
    }
}
