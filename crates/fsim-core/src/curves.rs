//! Кривые отклика: значение атрибута 0–1 → влияние 0–1. Вычисление через таблицу на 256
//! значений с линейной интерполяцией (`docs/01-data-model.md` §1).

use crate::attrs::{Attr, ATTR_COUNT};
use crate::config::{ConfigError, CurveSpec, CurvesConfig};

pub const TABLE_SIZE: usize = 256;

#[derive(Clone)]
pub struct CurveTable([f32; TABLE_SIZE]);

impl CurveTable {
    pub fn build(spec: &CurveSpec) -> CurveTable {
        let mut t = [0.0f32; TABLE_SIZE];
        let last = (TABLE_SIZE - 1) as f32;
        // Логистика нормируется по концам, чтобы кривая шла ровно из (0, 0) в (1, 1).
        let (lo, hi) = match spec {
            CurveSpec::Logistic { center, steepness } => (
                logistic(0.0, *center, *steepness),
                logistic(1.0, *center, *steepness),
            ),
            _ => (0.0, 1.0),
        };
        for (i, slot) in t.iter_mut().enumerate() {
            let x = i as f32 / last;
            *slot = match spec {
                CurveSpec::Linear => x,
                CurveSpec::Logistic { center, steepness } => {
                    (logistic(x, *center, *steepness) - lo) / (hi - lo)
                }
                CurveSpec::Piecewise { knots } => piecewise(knots, x),
            };
        }
        CurveTable(t)
    }

    #[inline]
    pub fn eval(&self, v: f32) -> f32 {
        let x = v.clamp(0.0, 1.0) * (TABLE_SIZE - 1) as f32;
        let i = (x as usize).min(TABLE_SIZE - 2);
        let f = x - i as f32;
        self.0[i] + (self.0[i + 1] - self.0[i]) * f
    }
}

fn logistic(x: f32, center: f32, steepness: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-steepness * (x - center)))
}

fn piecewise(knots: &[[f32; 2]], x: f32) -> f32 {
    for w in knots.windows(2) {
        if x <= w[1][0] {
            let f = (x - w[0][0]) / (w[1][0] - w[0][0]);
            return w[0][1] + (w[1][1] - w[0][1]) * f;
        }
    }
    knots[knots.len() - 1][1]
}

/// Таблица кривой для каждого атрибута: группа, затем переопределение по имени.
#[derive(Clone)]
pub struct CurveSet {
    tables: Vec<CurveTable>,
}

impl CurveSet {
    pub fn build(cfg: &CurvesConfig) -> Result<CurveSet, ConfigError> {
        for name in cfg.overrides.keys() {
            if Attr::from_name(name).is_none() {
                return Err(ConfigError::Invalid {
                    file: "response_curves.json".into(),
                    msg: format!("переопределение для неизвестного атрибута `{name}`"),
                });
            }
        }
        let mut tables = Vec::with_capacity(ATTR_COUNT);
        for &a in Attr::ALL {
            let spec = cfg
                .overrides
                .get(a.name())
                .or_else(|| cfg.groups.get(a.group()))
                .ok_or_else(|| ConfigError::Invalid {
                    file: "response_curves.json".into(),
                    msg: format!(
                        "нет кривой для группы `{}` (атрибут `{}`)",
                        a.group(),
                        a.name()
                    ),
                })?;
            tables.push(CurveTable::build(spec));
        }
        Ok(CurveSet { tables })
    }

    /// Влияние атрибута `a` при хранимом значении `v`.
    #[inline]
    pub fn eval(&self, a: Attr, v: f32) -> f32 {
        self.tables[a as usize].eval(v)
    }
}
