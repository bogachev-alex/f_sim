//! Конфиги: serde с `deny_unknown_fields`, проверка диапазонов, при ошибке падение.
//! Магических чисел в коде нет: все коэффициенты читаются из `config/`.

mod attributes;
mod composites;
mod curves;
mod decision;
mod defense;
mod formations;
mod perf;
mod physics;
mod positioning;
mod roles;
mod sandbox;
mod scenario;
mod styles;
mod xt;

pub use attributes::Attributes;
pub use composites::Composites;
pub use curves::{CurveSpec, CurvesConfig};
pub use decision::{
    Candidates, Commit, Control, DecisionConfig, Duel, PassModel, Pressure, Reaction, Restart,
    Save, Shot, Softmax, Utility, Xg,
};
pub use defense::{Assign, CounterPress, DefenseConfig, Marking, Owner, Press};
pub use formations::{Formations, Slot};
pub use perf::PerfConfig;
pub use physics::PhysicsConfig;
pub use positioning::{
    AttackShape, DefenseShape, KeeperShape, LineScatter, PhaseTiming, PositioningConfig, Steering,
};
pub use roles::{PhaseOffset, Role};
pub use sandbox::SandboxConfig;
pub use scenario::{BallPoint, PossessionChange, Scenario};
pub use styles::{FinalThird, FullbackRole, OverloadSide, PressTrigger, Style};
pub use xt::XtGrid;

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{file}: не удалось прочитать: {source}")]
    Io {
        file: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{file}: ошибка разбора JSON: {source}")]
    Parse {
        file: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{file}: поле `{field}` = {value}, допустимо [{min}, {max}]")]
    Range {
        file: String,
        field: String,
        value: f64,
        min: f64,
        max: f64,
    },
    #[error("{file}: {msg}")]
    Invalid { file: String, msg: String },
}

/// Проверка значения на принадлежность отрезку.
pub(crate) fn check_range(
    file: &str,
    field: &str,
    value: f64,
    min: f64,
    max: f64,
) -> Result<(), ConfigError> {
    // `!(..)` ловит и NaN.
    if !(min..=max).contains(&value) {
        return Err(ConfigError::Range {
            file: file.into(),
            field: field.into(),
            value,
            min,
            max,
        });
    }
    Ok(())
}

/// Проверка пары «мин ≤ макс».
pub(crate) fn check_order(
    file: &str,
    lo: &str,
    hi: &str,
    a: f64,
    b: f64,
) -> Result<(), ConfigError> {
    if a > b {
        return Err(ConfigError::Invalid {
            file: file.into(),
            msg: format!("`{lo}` ({a}) больше `{hi}` ({b})"),
        });
    }
    Ok(())
}

pub(crate) fn parse<T: serde::de::DeserializeOwned>(
    file: &str,
    json: &str,
) -> Result<T, ConfigError> {
    serde_json::from_str(json).map_err(|source| ConfigError::Parse {
        file: file.into(),
        source,
    })
}

/// Источник файлов конфига: каталог на диске или вшитые в сборку данные.
pub trait ConfigSource {
    /// Содержимое файла по относительному пути, например `physics.json`.
    fn read(&self, path: &str) -> Result<String, ConfigError>;
    /// Имена файлов `*.json` каталога без расширения, по возрастанию.
    fn list(&self, dir: &str) -> Result<Vec<String>, ConfigError>;
}

struct FsSource<'a>(&'a Path);

impl ConfigSource for FsSource<'_> {
    fn read(&self, path: &str) -> Result<String, ConfigError> {
        std::fs::read_to_string(self.0.join(path)).map_err(|source| ConfigError::Io {
            file: path.into(),
            source,
        })
    }

    fn list(&self, dir: &str) -> Result<Vec<String>, ConfigError> {
        let io = |source| ConfigError::Io {
            file: dir.into(),
            source,
        };
        let mut names = Vec::new();
        for entry in std::fs::read_dir(self.0.join(dir)).map_err(io)? {
            let path = entry.map_err(io)?.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

/// Все конфиги, загруженные и проверенные.
#[derive(Debug, Clone)]
pub struct Config {
    pub attributes: Attributes,
    pub composites: Composites,
    pub physics: PhysicsConfig,
    pub curves: CurvesConfig,
    pub formations: Formations,
    pub perf: PerfConfig,
    pub sandbox: SandboxConfig,
    pub positioning: PositioningConfig,
    pub defense: DefenseConfig,
    pub decision: DecisionConfig,
    pub xt: XtGrid,
    /// Стили по имени файла в `config/styles/`.
    pub styles: BTreeMap<String, Style>,
    /// Роли по имени файла в `config/roles/`.
    pub roles: BTreeMap<String, Role>,
    /// Сценарии по имени файла в `config/scenarios/`.
    pub scenarios: BTreeMap<String, Scenario>,
}

/// Идентификаторы ролей фланговых защитников, которые выбирает `Style::fullback_role`.
pub const FULLBACK_ROLE_OVERLAPPING: &str = "overlapping_fb";
pub const FULLBACK_ROLE_INVERTED: &str = "inverted_fb";
pub const FULLBACK_ROLE_STAY: &str = "defensive_fb";

impl Config {
    /// Загрузка из каталога `config/`.
    pub fn load_dir(dir: &Path) -> Result<Config, ConfigError> {
        Self::load_from(&FsSource(dir))
    }

    pub fn load_from(src: &dyn ConfigSource) -> Result<Config, ConfigError> {
        let attributes = Attributes::from_json(&src.read("attributes.json")?)?;
        let composites = Composites::from_json(&src.read("composites.json")?, &attributes)?;
        let physics = PhysicsConfig::from_json(&src.read("physics.json")?)?;
        let curves = CurvesConfig::from_json(&src.read("response_curves.json")?)?;
        let formations = Formations::from_json(&src.read("formations.json")?)?;
        let perf = PerfConfig::from_json(&src.read("perf.json")?)?;
        let sandbox = SandboxConfig::from_json(&src.read("sandbox.json")?)?;
        let positioning = PositioningConfig::from_json(&src.read("positioning.json")?)?;
        let defense = DefenseConfig::from_json(&src.read("defense.json")?)?;
        let decision = DecisionConfig::from_json(&src.read("decision.json")?)?;
        let xt = XtGrid::from_json(&src.read("xt_grid.json")?)?;

        let mut styles = BTreeMap::new();
        for name in src.list("styles")? {
            let file = format!("styles/{name}.json");
            styles.insert(name, Style::from_json(&file, &src.read(&file)?)?);
        }
        let mut roles = BTreeMap::new();
        for name in src.list("roles")? {
            let file = format!("roles/{name}.json");
            roles.insert(name, Role::from_json(&file, &src.read(&file)?)?);
        }
        let (hl, hw) = (physics.pitch.length_m * 0.5, physics.pitch.width_m * 0.5);
        let mut scenarios = BTreeMap::new();
        for name in src.list("scenarios")? {
            let file = format!("scenarios/{name}.json");
            scenarios.insert(name, Scenario::from_json(&file, &src.read(&file)?, hl, hw)?);
        }
        let cfg = Config {
            attributes,
            composites,
            physics,
            curves,
            formations,
            perf,
            sandbox,
            positioning,
            defense,
            decision,
            xt,
            styles,
            roles,
            scenarios,
        };
        cfg.cross_check()?;
        Ok(cfg)
    }

    /// Проверки связей между файлами: роли схем, схемы и стили стилей и сценариев.
    fn cross_check(&self) -> Result<(), ConfigError> {
        let bad = |file: &str, msg: String| {
            Err(ConfigError::Invalid {
                file: file.into(),
                msg,
            })
        };
        for id in [
            FULLBACK_ROLE_OVERLAPPING,
            FULLBACK_ROLE_INVERTED,
            FULLBACK_ROLE_STAY,
        ] {
            if !self.roles.contains_key(id) {
                return bad(
                    "roles",
                    format!("нет роли `{id}`, которую выбирает fullback_role"),
                );
            }
        }
        for (name, slots) in &self.formations.0 {
            for slot in slots {
                match self.roles.get(&slot.role) {
                    None => {
                        return bad(
                            "formations.json",
                            format!("{name}: неизвестная роль `{}`", slot.role),
                        )
                    }
                    Some(r) if !r.positions.contains(&slot.pos) => {
                        return bad(
                            "formations.json",
                            format!(
                                "{name}: роль `{}` не подходит позиции {:?}",
                                slot.role, slot.pos
                            ),
                        )
                    }
                    Some(_) => {}
                }
            }
        }
        for (name, st) in &self.styles {
            if !self.formations.0.contains_key(&st.default_formation) {
                return bad(
                    &format!("styles/{name}.json"),
                    format!("схема `{}` не найдена", st.default_formation),
                );
            }
        }
        for (name, sc) in &self.scenarios {
            let file = format!("scenarios/{name}.json");
            for f in &sc.formations {
                if !self.formations.0.contains_key(f) {
                    return bad(&file, format!("схема `{f}` не найдена"));
                }
            }
            for s in &sc.styles {
                if !self.styles.contains_key(s) {
                    return bad(&file, format!("стиль `{s}` не найден"));
                }
            }
        }
        Ok(())
    }
}
