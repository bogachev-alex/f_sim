use super::{check_range, parse, ConfigError};
use crate::data::{Position, STARTERS};
use serde::Deserialize;
use std::collections::BTreeMap;

const FILE: &str = "formations.json";

/// Слот схемы: позиция и базовая точка в долях поля (`x` от своих ворот к чужим, `y` слева направо).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub pos: Position,
    pub x: f32,
    pub y: f32,
    /// Идентификатор роли из `config/roles/`.
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct Formations(pub BTreeMap<String, Vec<Slot>>);

impl Formations {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let map: BTreeMap<String, Vec<Slot>> = parse(FILE, json)?;
        for (name, slots) in &map {
            let bad = |msg: String| ConfigError::Invalid {
                file: FILE.into(),
                msg: format!("`{name}`: {msg}"),
            };
            if slots.len() != STARTERS {
                return Err(bad(format!("{} слотов вместо {STARTERS}", slots.len())));
            }
            if slots.iter().filter(|s| s.pos == Position::GK).count() != 1
                || slots[0].pos != Position::GK
            {
                return Err(bad("первым слотом должен быть единственный GK".into()));
            }
            for s in slots {
                check_range(FILE, &format!("{name}.x"), s.x as f64, 0.0, 1.0)?;
                check_range(FILE, &format!("{name}.y"), s.y as f64, 0.0, 1.0)?;
            }
        }
        Ok(Formations(map))
    }
}
