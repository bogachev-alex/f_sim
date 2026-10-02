//! Состояние мира в SoA. Все буферы фиксированного размера, выделяются один раз.

use crate::physics::body::BodyParams;
use glam::Vec3;

pub const PLAYERS: usize = 22;

/// Игроки по полям: индексы 0–10 первая команда, 11–21 вторая.
#[derive(Clone)]
pub struct Players {
    pub pos_x: [f32; PLAYERS],
    pub pos_y: [f32; PLAYERS],
    pub vel_x: [f32; PLAYERS],
    pub vel_y: [f32; PLAYERS],
    pub facing: [f32; PLAYERS],
    pub target_x: [f32; PLAYERS],
    pub target_y: [f32; PLAYERS],
    /// Желаемая скорость (м/с), не выше максимальной скорости игрока.
    pub speed_cap: [f32; PLAYERS],
    /// Якорная точка (отладка и цель позиционирования).
    pub anchor_x: [f32; PLAYERS],
    pub anchor_y: [f32; PLAYERS],
    /// Обязанность (`defense::DUTY_*`) и игрок, к которому она относится (общий индекс или 255).
    pub duty: [u8; PLAYERS],
    pub duty_ref: [u8; PLAYERS],
}

/// Состояние команды для отладки: фаза, линия обороны, линия офсайда (мировая `x`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TeamState {
    /// Вес фазы атаки: 0 оборона, 1 атака.
    pub attack_w: f32,
    /// Согласованная линия обороны.
    pub line_x: f32,
    /// Линия офсайда: предпоследний игрок команды.
    pub offside_x: f32,
    pub has_ball: bool,
}

#[derive(Clone)]
pub struct World {
    pub players: Players,
    pub body: [BodyParams; PLAYERS],
    pub ball_pos: Vec3,
    pub ball_vel: Vec3,
    pub tick: u32,
    pub teams: [TeamState; 2],
    /// Состояние мяча для визуализатора: 0 по скрипту, 1 у ног, 2 в полёте, 3 ничей, 4 вне
    /// игры; игрок, к которому оно относится (общий индекс), или 255.
    pub ball_mode: u8,
    pub ball_actor: u8,
}

impl World {
    pub fn new(body: [BodyParams; PLAYERS]) -> Self {
        World {
            players: Players {
                pos_x: [0.0; PLAYERS],
                pos_y: [0.0; PLAYERS],
                vel_x: [0.0; PLAYERS],
                vel_y: [0.0; PLAYERS],
                facing: [0.0; PLAYERS],
                target_x: [0.0; PLAYERS],
                target_y: [0.0; PLAYERS],
                speed_cap: [0.0; PLAYERS],
                anchor_x: [0.0; PLAYERS],
                anchor_y: [0.0; PLAYERS],
                duty: [0; PLAYERS],
                duty_ref: [255; PLAYERS],
            },
            body,
            ball_pos: Vec3::ZERO,
            ball_vel: Vec3::ZERO,
            tick: 0,
            teams: [TeamState::default(); 2],
            ball_mode: 0,
            ball_actor: 255,
        }
    }
}
