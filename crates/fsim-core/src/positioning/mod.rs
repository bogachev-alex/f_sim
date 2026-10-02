//! Игра без мяча: якорные точки по схеме, фазе, стилю и роли; линия обороны; steering.
//! Целевая точка игрока складывается из якоря, смещения роли и поправок линии и компактности
//! (`docs/04-off-ball.md` §1, §3). На M2 мяч и владение заданы скриптом сценария.

pub mod anchors;
pub mod line;
pub mod steering;
pub mod team;

use glam::Vec2;

/// Перевод между мировыми координатами и системой команды: `X` от своей линии ворот к воротам
/// соперника (0..длина), `Y` положительный справа от атакующего. Команда 0 атакует в +x.
#[inline]
pub fn world_to_team(team: usize, p: Vec2, half_len: f32) -> Vec2 {
    if team == 0 {
        Vec2::new(p.x + half_len, p.y)
    } else {
        Vec2::new(half_len - p.x, -p.y)
    }
}

#[inline]
pub fn team_to_world(team: usize, p: Vec2, half_len: f32) -> Vec2 {
    if team == 0 {
        Vec2::new(p.x - half_len, p.y)
    } else {
        Vec2::new(half_len - p.x, -p.y)
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Перевод вектора (скорости, направления) между мировой системой и системой команды.
#[inline]
pub fn vec_to_team(team: usize, v: Vec2) -> Vec2 {
    if team == 0 {
        v
    } else {
        -v
    }
}
