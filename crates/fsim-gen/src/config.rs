//! Конфиг генератора (`config/generator.json`): сериализация, проверка, индексирование.

use fsim_core::attrs::Attr;
use fsim_core::data::{Position, POSITION_COUNT};
use serde::Deserialize;
use std::collections::BTreeMap;

const FILE: &str = "generator.json";
const GROUPS: [&str; 5] = ["physical", "technical", "mental", "goalkeeping", "traits"];

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("{FILE}: {0}")]
    Config(String),
    #[error("схема `{0}` не найдена в formations.json")]
    UnknownFormation(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeightGlobal {
    pub mean: f32,
    pub sd: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootProbs {
    #[serde(rename = "R")]
    pub r: f32,
    #[serde(rename = "L")]
    pub l: f32,
    #[serde(rename = "both")]
    pub both: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeakFoot {
    pub mean: f32,
    pub sd: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionProfile {
    pub group_offsets: BTreeMap<String, f32>,
    pub attr_offsets: BTreeMap<String, f32>,
    pub height_mean_cm: f32,
    pub height_sd_cm: f32,
    pub bmi_mean: f32,
    pub bmi_sd: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    default_quality: f32,
    attr_floor: f32,
    attr_ceil: f32,
    talent_sd: f32,
    noise_sd: BTreeMap<String, f32>,
    fixed_mean: BTreeMap<String, f32>,
    bench: Vec<Position>,
    bench_roles: BTreeMap<Position, String>,
    bench_quality_delta: f32,
    height_global: HeightGlobal,
    height_attr_shift_per_sd: BTreeMap<String, f32>,
    foot_probs: FootProbs,
    weak_foot: WeakFoot,
    affinity_default: f32,
    affinity: BTreeMap<Position, BTreeMap<Position, f32>>,
    positions: BTreeMap<Position, PositionProfile>,
}

#[derive(Debug, Clone)]
pub struct GenConfig {
    pub default_quality: f32,
    pub attr_floor: f32,
    pub attr_ceil: f32,
    pub talent_sd: f32,
    pub noise_sd: BTreeMap<String, f32>,
    pub fixed_mean: BTreeMap<String, f32>,
    pub bench: Vec<Position>,
    pub bench_roles: BTreeMap<Position, String>,
    pub bench_quality_delta: f32,
    pub height_global: HeightGlobal,
    pub height_shift: BTreeMap<String, f32>,
    pub foot_probs: FootProbs,
    pub weak_foot: WeakFoot,
    affinity_default: f32,
    affinity: BTreeMap<Position, BTreeMap<Position, f32>>,
    /// По порядку `Position::ALL`.
    pub positions: Vec<PositionProfile>,
}

fn bad<T>(msg: String) -> Result<T, GenError> {
    Err(GenError::Config(msg))
}

fn range(name: &str, v: f32, lo: f32, hi: f32) -> Result<(), GenError> {
    if !(lo..=hi).contains(&v) {
        return bad(format!("`{name}` = {v}, допустимо [{lo}, {hi}]"));
    }
    Ok(())
}

impl GenConfig {
    pub fn from_json(json: &str) -> Result<GenConfig, GenError> {
        let raw: Raw = serde_json::from_str(json)
            .map_err(|e| GenError::Config(format!("разбор JSON: {e}")))?;
        for pos in &raw.bench {
            if !raw.bench_roles.contains_key(pos) {
                return bad(format!("нет роли запасного для позиции {pos:?}"));
            }
        }
        range("default_quality", raw.default_quality, 0.0, 1.0)?;
        range("attr_floor", raw.attr_floor, 0.0, 0.5)?;
        range("attr_ceil", raw.attr_ceil, 0.5, 1.0)?;
        range("talent_sd", raw.talent_sd, 0.0, 0.3)?;
        range("bench_quality_delta", raw.bench_quality_delta, -0.5, 0.5)?;
        range("height_global.sd", raw.height_global.sd, 1.0, 20.0)?;
        range("affinity_default", raw.affinity_default, 0.0, 1.0)?;
        range("weak_foot.mean", raw.weak_foot.mean, 0.0, 1.0)?;
        range("weak_foot.sd", raw.weak_foot.sd, 0.0, 0.5)?;
        let fp = &raw.foot_probs;
        if (fp.r + fp.l + fp.both - 1.0).abs() > 1e-4 {
            return bad("`foot_probs` в сумме не дают 1".into());
        }
        for (g, v) in raw.noise_sd.iter().chain(&raw.fixed_mean) {
            if !GROUPS.contains(&g.as_str()) {
                return bad(format!("неизвестная группа `{g}`"));
            }
            range(g, *v, 0.0, 1.0)?;
        }
        for g in GROUPS {
            if !raw.noise_sd.contains_key(g) {
                return bad(format!("нет `noise_sd.{g}`"));
            }
        }
        let check_attrs = |what: &str, m: &BTreeMap<String, f32>| -> Result<(), GenError> {
            for (k, v) in m {
                if Attr::from_name(k).is_none() {
                    return bad(format!("{what}: неизвестный атрибут `{k}`"));
                }
                range(&format!("{what}.{k}"), *v, -0.5, 0.5)?;
            }
            Ok(())
        };
        check_attrs("height_attr_shift_per_sd", &raw.height_attr_shift_per_sd)?;

        let mut positions = Vec::with_capacity(POSITION_COUNT);
        for pos in Position::ALL {
            let prof = raw
                .positions
                .get(&pos)
                .ok_or_else(|| GenError::Config(format!("нет профиля позиции {pos:?}")))?
                .clone();
            check_attrs(&format!("{pos:?}.attr_offsets"), &prof.attr_offsets)?;
            for (g, v) in &prof.group_offsets {
                if !GROUPS.contains(&g.as_str()) {
                    return bad(format!("{pos:?}: неизвестная группа `{g}`"));
                }
                range(&format!("{pos:?}.group_offsets.{g}"), *v, -1.0, 1.0)?;
            }
            range(
                &format!("{pos:?}.height_mean_cm"),
                prof.height_mean_cm,
                150.0,
                210.0,
            )?;
            range(
                &format!("{pos:?}.height_sd_cm"),
                prof.height_sd_cm,
                0.0,
                15.0,
            )?;
            range(&format!("{pos:?}.bmi_mean"), prof.bmi_mean, 18.0, 28.0)?;
            range(&format!("{pos:?}.bmi_sd"), prof.bmi_sd, 0.0, 4.0)?;
            positions.push(prof);
        }
        for (from, row) in &raw.affinity {
            for (to, v) in row {
                range(&format!("affinity.{from:?}.{to:?}"), *v, 0.0, 1.0)?;
            }
        }
        Ok(GenConfig {
            default_quality: raw.default_quality,
            attr_floor: raw.attr_floor,
            attr_ceil: raw.attr_ceil,
            talent_sd: raw.talent_sd,
            noise_sd: raw.noise_sd,
            fixed_mean: raw.fixed_mean,
            bench: raw.bench,
            bench_roles: raw.bench_roles,
            bench_quality_delta: raw.bench_quality_delta,
            height_global: raw.height_global,
            height_shift: raw.height_attr_shift_per_sd,
            foot_probs: raw.foot_probs,
            weak_foot: raw.weak_foot,
            affinity_default: raw.affinity_default,
            affinity: raw.affinity,
            positions,
        })
    }

    /// Рейтинг игрока позиции `own` на позиции `other` (не своей).
    pub fn affinity_between(&self, own: Position, other: Position) -> f32 {
        self.affinity
            .get(&own)
            .and_then(|row| row.get(&other))
            .copied()
            .unwrap_or(self.affinity_default)
    }
}
