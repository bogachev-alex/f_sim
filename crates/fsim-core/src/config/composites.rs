use super::{check_range, parse, Attributes, ConfigError};
use std::collections::BTreeMap;

const FILE: &str = "composites.json";
/// Допуск на сумму весов композита (веса хранятся в `f32`).
const WEIGHT_SUM_TOL: f64 = 1e-5;

/// Навык → (атрибут → вес). `BTreeMap`: порядок обхода фиксирован.
#[derive(Debug, Clone)]
pub struct Composites(pub BTreeMap<String, BTreeMap<String, f32>>);

impl Composites {
    pub fn from_json(json: &str, attrs: &Attributes) -> Result<Self, ConfigError> {
        let map: BTreeMap<String, BTreeMap<String, f32>> = parse(FILE, json)?;
        for (skill, weights) in &map {
            let mut sum = 0.0f64;
            for (attr, &w) in weights {
                if !attrs.contains(attr) {
                    return Err(ConfigError::Invalid {
                        file: FILE.into(),
                        msg: format!("навык `{skill}`: неизвестный атрибут `{attr}`"),
                    });
                }
                check_range(FILE, &format!("{skill}.{attr}"), w as f64, 0.0, 1.0)?;
                sum += w as f64;
            }
            if (sum - 1.0).abs() > WEIGHT_SUM_TOL {
                return Err(ConfigError::Invalid {
                    file: FILE.into(),
                    msg: format!("навык `{skill}`: сумма весов {sum}, ожидается 1"),
                });
            }
        }
        Ok(Composites(map))
    }
}
