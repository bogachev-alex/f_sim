use super::{parse, ConfigError};
use serde::Deserialize;
use std::collections::BTreeSet;

const FILE: &str = "attributes.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    pub physical: Vec<String>,
    pub technical: Vec<String>,
    pub mental: Vec<String>,
    pub goalkeeping: Vec<String>,
    pub traits: Vec<String>,
}

impl Attributes {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let a: Attributes = parse(FILE, json)?;
        let mut seen = BTreeSet::new();
        for name in a.all() {
            if !seen.insert(name) {
                return Err(ConfigError::Invalid {
                    file: FILE.into(),
                    msg: format!("атрибут `{name}` повторяется"),
                });
            }
        }
        Ok(a)
    }

    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.physical
            .iter()
            .chain(&self.technical)
            .chain(&self.mental)
            .chain(&self.goalkeeping)
            .chain(&self.traits)
            .map(String::as_str)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.all().any(|a| a == name)
    }
}
