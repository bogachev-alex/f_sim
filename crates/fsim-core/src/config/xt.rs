use super::{check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "xt_grid.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    cols: usize,
    rows: usize,
    #[allow(dead_code)]
    description: String,
    values: Vec<Vec<f32>>,
}

/// Сетка ценности xT: колонки от своих ворот к чужим, строки слева направо.
#[derive(Debug, Clone)]
pub struct XtGrid {
    pub cols: usize,
    pub rows: usize,
    /// Построчно: `values[row * cols + col]`.
    pub values: Vec<f32>,
}

impl XtGrid {
    pub fn from_json(json: &str) -> Result<XtGrid, ConfigError> {
        let r: Raw = parse(FILE, json)?;
        let bad = |msg: String| ConfigError::Invalid {
            file: FILE.into(),
            msg,
        };
        if r.values.len() != r.rows || r.values.iter().any(|row| row.len() != r.cols) {
            return Err(bad(format!("сетка должна быть {} × {}", r.cols, r.rows)));
        }
        for row in &r.values {
            for &v in row {
                check_range(FILE, "xt", v as f64, 0.0, 1.0)?;
            }
        }
        Ok(XtGrid {
            cols: r.cols,
            rows: r.rows,
            values: r.values.into_iter().flatten().collect(),
        })
    }
}
