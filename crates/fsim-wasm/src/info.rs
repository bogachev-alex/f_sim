//! Описание матча и раскладка кадра. Типы экспортируются в TypeScript через `ts-rs`.

use fsim_core::config::{Config, Slot};
use fsim_core::data::Squad;
use fsim_core::physics::body::BodyParams;
use fsim_core::world::PLAYERS;
use serde::Serialize;
use ts_rs::TS;

/// Числа на игрока в кадре: `x, y, vx, vy, facing, target_x, target_y, speed_cap, anchor_x,
/// anchor_y, duty, duty_ref` (обязанность и игрок, к которому она относится, или 255).
pub const PLAYER_FIELDS: u32 = 12;
/// Числа на команду: `attack_w, line_x, offside_x, has_ball`.
pub const TEAM_FIELDS: u32 = 4;
/// Числа мяча в кадре: `x, y, z, vx, vy, vz, mode, actor` (режим мяча и игрок, 255 если нет).
pub const BALL_FIELDS: u32 = 8;

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct FrameLayout {
    /// Чисел в кадре: тик, игроки, команды, мяч.
    pub stride: u32,
    pub players: u32,
    pub player_fields: u32,
    pub team_fields: u32,
    pub ball_fields: u32,
    pub players_offset: u32,
    pub teams_offset: u32,
    pub ball_offset: u32,
}

impl FrameLayout {
    pub fn new() -> FrameLayout {
        let players = PLAYERS as u32;
        let players_offset = 1;
        let teams_offset = players_offset + players * PLAYER_FIELDS;
        let ball_offset = teams_offset + 2 * TEAM_FIELDS;
        FrameLayout {
            stride: ball_offset + BALL_FIELDS,
            players,
            player_fields: PLAYER_FIELDS,
            team_fields: TEAM_FIELDS,
            ball_fields: BALL_FIELDS,
            players_offset,
            teams_offset,
            ball_offset,
        }
    }
}

impl Default for FrameLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PitchInfo {
    pub length_m: f32,
    pub width_m: f32,
    pub goal_width_m: f32,
    pub goal_height_m: f32,
    pub goal_depth_m: f32,
    pub player_radius_m: f32,
    pub ball_radius_m: f32,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct PlayerInfo {
    /// 0 или 1.
    pub team: u8,
    pub number: u8,
    pub name: String,
    pub position: String,
    pub role: String,
    /// Линия: 0 вратарь, 1 защита, 2 полузащита, 3 атака.
    pub line: u8,
    pub height_cm: f32,
    pub max_speed_ms: f32,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct MatchInfo {
    #[ts(type = "string")]
    pub seed: String,
    /// `sandbox`, `scenario` или `match`.
    pub mode: String,
    pub scenario: Option<String>,
    /// Длительность сценария в тиках; 0 у песочницы.
    pub duration_ticks: u32,
    pub tick_hz: u32,
    pub gait_walk_fraction: f32,
    pub gait_jog_fraction: f32,
    pub formations: [String; 2],
    pub styles: [String; 2],
    pub layout: FrameLayout,
    pub pitch: PitchInfo,
    pub players: Vec<PlayerInfo>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ScenarioEntry {
    pub id: String,
    pub description: String,
    pub formations: [String; 2],
    pub styles: [String; 2],
    pub duration_s: f32,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct StyleEntry {
    pub id: String,
    pub description: String,
    pub default_formation: String,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Catalog {
    pub formations: Vec<String>,
    pub styles: Vec<StyleEntry>,
    pub scenarios: Vec<ScenarioEntry>,
}

impl Catalog {
    pub fn build(cfg: &Config) -> Catalog {
        Catalog {
            formations: cfg.formations.0.keys().cloned().collect(),
            styles: cfg
                .styles
                .iter()
                .map(|(id, s)| StyleEntry {
                    id: id.clone(),
                    description: s.description.clone(),
                    default_formation: s.default_formation.clone(),
                })
                .collect(),
            scenarios: cfg
                .scenarios
                .iter()
                .map(|(id, s)| ScenarioEntry {
                    id: id.clone(),
                    description: s.description.clone(),
                    formations: s.formations.clone(),
                    styles: s.styles.clone(),
                    duration_s: s.duration_s,
                })
                .collect(),
        }
    }
}

/// Линии схемы: слоты группируются в ряды по глубине (разрыв больше порога начинает новый
/// ряд); вратарь 0, первый ряд защита, последний атака, остальные полузащита.
pub fn line_ranks(slots: &[Slot], row_gap: f32) -> Vec<u8> {
    let mut xs: Vec<f32> = slots
        .iter()
        .filter(|s| s.pos != fsim_core::data::Position::GK)
        .map(|s| s.x)
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Ряд начинается там, где соседние по глубине слоты расходятся больше порога.
    let mut rows: Vec<f32> = Vec::new();
    let mut prev = f32::MIN;
    for x in xs {
        if x - prev > row_gap {
            rows.push(x);
        }
        prev = x;
    }
    slots
        .iter()
        .map(|s| {
            if s.pos == fsim_core::data::Position::GK {
                return 0;
            }
            let row = rows.iter().rposition(|&r| s.x >= r).unwrap_or(0);
            if row == 0 {
                1
            } else if row == rows.len() - 1 {
                3
            } else {
                2
            }
        })
        .collect()
}

pub struct InfoParams<'a> {
    pub mode: &'a str,
    pub seed: u64,
    pub scenario: Option<&'a str>,
    pub duration_ticks: u32,
    pub styles: [String; 2],
    pub squads: [&'a Squad; 2],
    pub roles: [Vec<String>; 2],
}

/// Порог разрыва по глубине между рядами схемы, в долях длины поля (визуальная группировка).
const ROW_GAP: f32 = 0.12;

impl MatchInfo {
    pub fn build(cfg: &Config, p: &InfoParams, body: &[BodyParams; PLAYERS]) -> MatchInfo {
        let mut players = Vec::with_capacity(PLAYERS);
        for (team, squad) in p.squads.iter().enumerate() {
            let slots = &cfg.formations.0[&squad.formation];
            let ranks = line_ranks(slots, ROW_GAP);
            for (i, pl) in squad.players.iter().take(11).enumerate() {
                players.push(PlayerInfo {
                    team: team as u8,
                    number: (i + 1) as u8,
                    name: pl.name.clone(),
                    position: format!("{:?}", pl.position),
                    role: p.roles[team][i].clone(),
                    line: ranks[i],
                    height_cm: pl.height_cm,
                    max_speed_ms: body[team * 11 + i].max_speed,
                });
            }
        }
        MatchInfo {
            seed: p.seed.to_string(),
            mode: p.mode.to_string(),
            scenario: p.scenario.map(str::to_owned),
            duration_ticks: p.duration_ticks,
            tick_hz: cfg.physics.time.tick_hz,
            gait_walk_fraction: cfg.physics.player.gait_walk_fraction,
            gait_jog_fraction: cfg.physics.player.gait_jog_fraction,
            formations: [p.squads[0].formation.clone(), p.squads[1].formation.clone()],
            styles: p.styles.clone(),
            layout: FrameLayout::new(),
            pitch: PitchInfo {
                length_m: cfg.physics.pitch.length_m,
                width_m: cfg.physics.pitch.width_m,
                goal_width_m: cfg.physics.goal.width_m,
                goal_height_m: cfg.physics.goal.height_m,
                goal_depth_m: cfg.physics.goal.depth_m,
                player_radius_m: cfg.physics.player.radius_m,
                ball_radius_m: cfg.physics.ball.radius_m,
            },
            players,
        }
    }
}
