use super::{check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "sandbox.json";

/// Параметры песочницы M1: игроки бегут к случайным точкам, мяч периодически бьют.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    pub retarget_period_s: f32,
    pub kick_period_s: f32,
    /// Доля вертикальной составляющей скорости удара от горизонтальной.
    pub kick_lift_max_ratio: f32,
    pub wind_x_ms: f32,
    pub wind_y_ms: f32,
    pub rain: f32,
    pub pitch_bounce: f32,
    /// Значение всех атрибутов игроков в песочнице.
    pub uniform_attr: f32,
}

impl SandboxConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: SandboxConfig = parse(FILE, json)?;
        let f = |n: &str, v: f32, lo: f64, hi: f64| check_range(FILE, n, v as f64, lo, hi);
        f("retarget_period_s", c.retarget_period_s, 0.1, 60.0)?;
        f("kick_period_s", c.kick_period_s, 0.1, 60.0)?;
        f("kick_lift_max_ratio", c.kick_lift_max_ratio, 0.0, 2.0)?;
        f("wind_x_ms", c.wind_x_ms, -20.0, 20.0)?;
        f("wind_y_ms", c.wind_y_ms, -20.0, 20.0)?;
        f("rain", c.rain, 0.0, 1.0)?;
        f("pitch_bounce", c.pitch_bounce, 0.0, 1.0)?;
        f("uniform_attr", c.uniform_attr, 0.0, 1.0)?;
        Ok(c)
    }
}
