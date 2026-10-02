use super::{check_range, parse, ConfigError};
use serde::Deserialize;

/// Сторона перегруза (`overload_side`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverloadSide {
    Left,
    Center,
    Right,
    None,
}

/// Роль фланговых защитников в стиле.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FullbackRole {
    Overlapping,
    Inverted,
    Stay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressTrigger {
    BackPass,
    BadTouch,
    Touchline,
    BackToGoal,
    PassToKeeper,
}

/// Веса действий в финальной трети; в сумме дают 1.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalThird {
    pub crosses: f32,
    pub through: f32,
    pub long_shots: f32,
    pub combinations: f32,
}

/// Параметры командного стиля (`docs/05-styles.md` §1), все числа 0–1.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub name: String,
    pub description: String,
    pub default_formation: String,
    pub buildup_short: f32,
    pub tempo: f32,
    pub directness: f32,
    pub width_in: f32,
    pub overload_side: OverloadSide,
    pub final_third: FinalThird,
    pub patience: f32,
    pub fullback_role: FullbackRole,
    pub block_height: f32,
    pub line_height: f32,
    pub compactness: f32,
    pub press_intensity: f32,
    pub press_triggers: Vec<PressTrigger>,
    pub marking: f32,
    pub offside_trap: f32,
    pub counter_press: f32,
    pub counter_attack: f32,
}

impl Style {
    pub fn from_json(file: &str, json: &str) -> Result<Style, ConfigError> {
        let s: Style = parse(file, json)?;
        let unit = [
            ("buildup_short", s.buildup_short),
            ("tempo", s.tempo),
            ("directness", s.directness),
            ("width_in", s.width_in),
            ("patience", s.patience),
            ("block_height", s.block_height),
            ("line_height", s.line_height),
            ("compactness", s.compactness),
            ("press_intensity", s.press_intensity),
            ("marking", s.marking),
            ("offside_trap", s.offside_trap),
            ("counter_press", s.counter_press),
            ("counter_attack", s.counter_attack),
            ("final_third.crosses", s.final_third.crosses),
            ("final_third.through", s.final_third.through),
            ("final_third.long_shots", s.final_third.long_shots),
            ("final_third.combinations", s.final_third.combinations),
        ];
        for (name, v) in unit {
            check_range(file, name, v as f64, 0.0, 1.0)?;
        }
        let f = &s.final_third;
        let sum = f.crosses + f.through + f.long_shots + f.combinations;
        if (sum - 1.0).abs() > 1e-4 {
            return Err(ConfigError::Invalid {
                file: file.into(),
                msg: format!("веса final_third в сумме {sum}, ожидается 1"),
            });
        }
        let mut seen = s.press_triggers.clone();
        seen.sort();
        seen.dedup();
        if seen.len() != s.press_triggers.len() {
            return Err(ConfigError::Invalid {
                file: file.into(),
                msg: "press_triggers содержит повторы".into(),
            });
        }
        Ok(s)
    }
}
