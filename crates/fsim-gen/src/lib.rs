//! Генератор тестовых составов (`docs/01-data-model.md` §9).
//!
//! Игрок строится от архетипа позиции: средний уровень состава плюс смещения позиции, общий
//! «талант» игрока, шум и корреляция с ростом (высокий выше в `jumping_reach` и `strength`,
//! ниже в `agility`). Режимы «состав под стиль» и «против стиля» появятся вместе со стилями
//! (M9, M10): они задаются смещениями атрибутов, которые принимает `SquadParams::bias`.

mod config;

pub use config::{GenConfig, GenError};

use fsim_core::attrs::{Attr, ATTR_COUNT};
use fsim_core::config::{Formations, Slot};
use fsim_core::data::{Foot, PlayerData, Position, Squad, POSITION_COUNT};
use fsim_core::rng::{splitmix64, Rng};

/// Параметры одного состава.
#[derive(Clone, Debug)]
pub struct SquadParams {
    pub seed: u64,
    pub name: String,
    pub formation: String,
    /// Общее качество 0–1; `None` берёт `default_quality` из конфига.
    pub quality: Option<f32>,
    /// Смещение атрибутов для режимов «под стиль» и «против стиля».
    pub bias: Vec<(Attr, f32)>,
}

impl SquadParams {
    pub fn new(seed: u64, name: &str, formation: &str) -> Self {
        SquadParams {
            seed,
            name: name.into(),
            formation: formation.into(),
            quality: None,
            bias: Vec::new(),
        }
    }
}

pub fn generate_squad(
    cfg: &GenConfig,
    formations: &Formations,
    p: &SquadParams,
) -> Result<Squad, GenError> {
    let slots: &Vec<Slot> = formations
        .0
        .get(&p.formation)
        .ok_or_else(|| GenError::UnknownFormation(p.formation.clone()))?;
    let quality = p.quality.unwrap_or(cfg.default_quality);
    let mut players = Vec::with_capacity(slots.len() + cfg.bench.len());
    let mut rng = Rng::new(p.seed, 0);
    for (i, slot) in slots.iter().enumerate() {
        let tag = format!("{}{:02}", p.name, i + 1);
        players.push(gen_player(
            cfg, &mut rng, slot.pos, &slot.role, quality, &p.bias, tag,
        ));
    }
    for (i, &pos) in cfg.bench.iter().enumerate() {
        let tag = format!("{}{:02}", p.name, slots.len() + i + 1);
        let q = quality + cfg.bench_quality_delta;
        players.push(gen_player(
            cfg,
            &mut rng,
            pos,
            &cfg.bench_roles[&pos],
            q,
            &p.bias,
            tag,
        ));
    }
    Ok(Squad {
        name: p.name.clone(),
        formation: p.formation.clone(),
        quality,
        players,
    })
}

/// Режим «одинаковые составы»: две команды из одного распределения, разные случайные игроки.
pub fn generate_equal_pair(
    cfg: &GenConfig,
    formations: &Formations,
    seed: u64,
    formation: &str,
    quality: Option<f32>,
) -> Result<(Squad, Squad), GenError> {
    let mut sm = seed;
    let (sa, sb) = (splitmix64(&mut sm), splitmix64(&mut sm));
    let mut a = SquadParams::new(sa, "A", formation);
    let mut b = SquadParams::new(sb, "B", formation);
    a.quality = quality;
    b.quality = quality;
    Ok((
        generate_squad(cfg, formations, &a)?,
        generate_squad(cfg, formations, &b)?,
    ))
}

fn gen_player(
    cfg: &GenConfig,
    rng: &mut Rng,
    pos: Position,
    role: &str,
    quality: f32,
    bias: &[(Attr, f32)],
    name: String,
) -> PlayerData {
    let prof = &cfg.positions[pos as usize];
    let talent = rng.next_normal() * cfg.talent_sd;
    let height = prof.height_mean_cm + rng.next_normal() * prof.height_sd_cm;
    let h_z = (height - cfg.height_global.mean) / cfg.height_global.sd;
    let bmi = prof.bmi_mean + rng.next_normal() * prof.bmi_sd;
    let h_m = height / 100.0;

    let mut attrs = vec![0.0f32; ATTR_COUNT];
    for &a in Attr::ALL {
        let g = a.group();
        let mean = match cfg.fixed_mean.get(g) {
            Some(&m) => m,
            None => {
                quality
                    + talent
                    + prof.group_offsets.get(g).copied().unwrap_or(0.0)
                    + prof.attr_offsets.get(a.name()).copied().unwrap_or(0.0)
                    + cfg.height_shift.get(a.name()).copied().unwrap_or(0.0) * h_z
                    + bias
                        .iter()
                        .filter(|(b, _)| *b == a)
                        .map(|(_, d)| d)
                        .sum::<f32>()
            }
        };
        let sd = cfg.noise_sd.get(g).copied().unwrap_or(0.0);
        attrs[a as usize] = (mean + rng.next_normal() * sd).clamp(cfg.attr_floor, cfg.attr_ceil);
    }

    let foot = {
        let u = rng.next_f32();
        if u < cfg.foot_probs.r {
            Foot::R
        } else if u < cfg.foot_probs.r + cfg.foot_probs.l {
            Foot::L
        } else {
            Foot::Both
        }
    };
    let weak_foot = if foot == Foot::Both {
        1.0
    } else {
        (cfg.weak_foot.mean + rng.next_normal() * cfg.weak_foot.sd).clamp(0.0, 1.0)
    };
    let mut ratings = vec![0.0f32; POSITION_COUNT];
    for (i, &p) in Position::ALL.iter().enumerate() {
        ratings[i] = if p == pos {
            1.0
        } else {
            cfg.affinity_between(pos, p)
        };
    }
    PlayerData {
        name,
        attrs,
        height_cm: height,
        weight_kg: bmi * h_m * h_m,
        preferred_foot: foot,
        weak_foot,
        position_ratings: ratings,
        position: pos,
        role: role.to_string(),
    }
}
