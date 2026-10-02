use super::{check_range, parse, ConfigError};
use crate::data::Position;
use serde::Deserialize;

/// Смещение якорной точки роли в одной фазе, в метрах в системе команды.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseOffset {
    /// Вдоль поля, положительное к воротам соперника.
    pub dx_m: f32,
    /// К центру поля, отрицательное шире.
    pub inward_m: f32,
}

/// Роль игрока (`docs/01-data-model.md` §4). Параметры рывков, бонусы полезности и
/// обязанности в обороне добавятся на этапах M3–M5.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub name: String,
    pub positions: Vec<Position>,
    pub description: String,
    pub attack: PhaseOffset,
    pub defense: PhaseOffset,
}

/// Предел смещения роли: больше уже не роль, а другая позиция.
const MAX_OFFSET_M: f64 = 25.0;

impl Role {
    pub fn from_json(file: &str, json: &str) -> Result<Role, ConfigError> {
        let r: Role = parse(file, json)?;
        if r.positions.is_empty() {
            return Err(ConfigError::Invalid {
                file: file.into(),
                msg: "пустой список позиций".into(),
            });
        }
        for (n, o) in [("attack", r.attack), ("defense", r.defense)] {
            check_range(
                file,
                &format!("{n}.dx_m"),
                o.dx_m as f64,
                -MAX_OFFSET_M,
                MAX_OFFSET_M,
            )?;
            check_range(
                file,
                &format!("{n}.inward_m"),
                o.inward_m as f64,
                -MAX_OFFSET_M,
                MAX_OFFSET_M,
            )?;
        }
        Ok(r)
    }
}
