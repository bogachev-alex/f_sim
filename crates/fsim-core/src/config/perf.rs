use super::{check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "perf.json";

/// Бюджеты производительности (`docs/02-engine-core.md`, раздел 7).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerfConfig {
    pub match_budget_s: f64,
    pub batch_matches: u32,
    pub batch_cores: u32,
    pub batch_budget_min: f64,
    pub tick_allocs_max: u32,
    pub full_match_regression_pct: f64,
    pub browser_speedup: u32,
}

impl PerfConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: PerfConfig = parse(FILE, json)?;
        check_range(FILE, "match_budget_s", c.match_budget_s, 0.001, 60.0)?;
        check_range(FILE, "batch_matches", c.batch_matches as f64, 1.0, 1e7)?;
        check_range(FILE, "batch_cores", c.batch_cores as f64, 1.0, 1024.0)?;
        check_range(FILE, "batch_budget_min", c.batch_budget_min, 0.1, 1440.0)?;
        check_range(FILE, "tick_allocs_max", c.tick_allocs_max as f64, 0.0, 1e6)?;
        check_range(
            FILE,
            "full_match_regression_pct",
            c.full_match_regression_pct,
            0.0,
            100.0,
        )?;
        check_range(
            FILE,
            "browser_speedup",
            c.browser_speedup as f64,
            1.0,
            1000.0,
        )?;
        Ok(c)
    }
}
