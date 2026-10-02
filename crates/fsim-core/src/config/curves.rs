use super::{check_range, parse, ConfigError};
use serde::Deserialize;
use std::collections::BTreeMap;

const FILE: &str = "response_curves.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveSpec {
    Linear,
    Logistic { center: f32, steepness: f32 },
    Piecewise { knots: Vec<[f32; 2]> },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvesConfig {
    pub groups: BTreeMap<String, CurveSpec>,
    pub overrides: BTreeMap<String, CurveSpec>,
}

impl CurvesConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: CurvesConfig = parse(FILE, json)?;
        for (name, spec) in c.groups.iter().chain(&c.overrides) {
            validate_spec(name, spec)?;
        }
        Ok(c)
    }
}

fn validate_spec(name: &str, spec: &CurveSpec) -> Result<(), ConfigError> {
    let bad = |msg: String| ConfigError::Invalid {
        file: FILE.into(),
        msg: format!("`{name}`: {msg}"),
    };
    match spec {
        CurveSpec::Linear => Ok(()),
        CurveSpec::Logistic { center, steepness } => {
            check_range(FILE, &format!("{name}.center"), *center as f64, 0.0, 1.0)?;
            check_range(
                FILE,
                &format!("{name}.steepness"),
                *steepness as f64,
                0.1,
                30.0,
            )
        }
        CurveSpec::Piecewise { knots } => {
            if knots.len() < 2 {
                return Err(bad("нужно минимум 2 узла".into()));
            }
            if knots[0][0] != 0.0 || knots[knots.len() - 1][0] != 1.0 {
                return Err(bad(
                    "узлы должны начинаться с x=0 и заканчиваться x=1".into()
                ));
            }
            for w in knots.windows(2) {
                if w[1][0] <= w[0][0] {
                    return Err(bad("x узлов должен строго возрастать".into()));
                }
                if w[1][1] < w[0][1] {
                    return Err(bad("y узлов не должен убывать".into()));
                }
            }
            for k in knots {
                check_range(FILE, &format!("{name}.knot"), k[1] as f64, 0.0, 1.0)?;
            }
            Ok(())
        }
    }
}
