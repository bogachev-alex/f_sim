//! Данные игроков и составов (`docs/01-data-model.md` §2, §6).

use crate::attrs::{Attr, Attrs, ATTR_COUNT};
use serde::{Deserialize, Serialize};

pub const POSITION_COUNT: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Position {
    GK,
    CB,
    FB,
    WB,
    DM,
    CM,
    AM,
    W,
    ST,
}

impl Position {
    pub const ALL: [Position; POSITION_COUNT] = [
        Position::GK,
        Position::CB,
        Position::FB,
        Position::WB,
        Position::DM,
        Position::CM,
        Position::AM,
        Position::W,
        Position::ST,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Foot {
    L,
    R,
    #[serde(rename = "both")]
    Both,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerData {
    pub name: String,
    /// Атрибуты 0–1 в порядке `Attr`.
    pub attrs: Vec<f32>,
    pub height_cm: f32,
    pub weight_kg: f32,
    pub preferred_foot: Foot,
    pub weak_foot: f32,
    /// Рейтинг на позиции 0–1 в порядке `Position::ALL`.
    pub position_ratings: Vec<f32>,
    /// Основная позиция в составе.
    pub position: Position,
    /// Роль из `config/roles/` (`docs/01-data-model.md` §4).
    pub role: String,
}

impl PlayerData {
    pub fn attr(&self, a: Attr) -> f32 {
        self.attrs[a as usize]
    }

    pub fn attrs_array(&self) -> Attrs {
        let mut out = [0.0; ATTR_COUNT];
        out.copy_from_slice(&self.attrs);
        out
    }

    /// Шкала для отображения: `round(1 + 99 v)`.
    pub fn display(v: f32) -> u8 {
        libm::roundf(1.0 + 99.0 * v) as u8
    }
}

/// Состав: 11 стартовых в порядке слотов схемы, затем запасные.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Squad {
    pub name: String,
    pub formation: String,
    pub quality: f32,
    pub players: Vec<PlayerData>,
}

pub const STARTERS: usize = 11;
