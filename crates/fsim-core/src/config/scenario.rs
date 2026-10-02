use super::{check_range, parse, ConfigError};
use serde::Deserialize;

/// Точка траектории мяча в мировых координатах (команда 0 атакует в +x).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BallPoint {
    pub t: f32,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub z: f32,
}

/// Смена владения: с момента `t` мячом владеет команда `team`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PossessionChange {
    pub t: f32,
    pub team: u8,
}

/// Сценарий: сохранённое состояние поля и скрипт мяча (`docs/07-validation.md` §10).
/// На M2 мяч идёт по скрипту, владение задано заранее; позиции игроков по умолчанию
/// берутся из якорей на начало сценария.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    pub duration_s: f32,
    pub formations: [String; 2],
    pub styles: [String; 2],
    pub possession: Vec<PossessionChange>,
    pub ball: Vec<BallPoint>,
    /// Счёт и минута: хранятся для сценариев с игровым контекстом (M6).
    pub score: [u8; 2],
    pub minute: u8,
}

impl Scenario {
    pub fn from_json(
        file: &str,
        json: &str,
        half_len: f32,
        half_wid: f32,
    ) -> Result<Scenario, ConfigError> {
        let s: Scenario = parse(file, json)?;
        let bad = |msg: String| ConfigError::Invalid {
            file: file.into(),
            msg,
        };
        check_range(file, "duration_s", s.duration_s as f64, 1.0, 7200.0)?;
        if s.ball.len() < 2 {
            return Err(bad("в траектории мяча нужно минимум 2 точки".into()));
        }
        if s.ball[0].t != 0.0 {
            return Err(bad("траектория мяча должна начинаться с t = 0".into()));
        }
        for w in s.ball.windows(2) {
            if w[1].t <= w[0].t {
                return Err(bad("время точек мяча должно строго возрастать".into()));
            }
        }
        if s.ball[s.ball.len() - 1].t < s.duration_s {
            return Err(bad("траектория мяча короче сценария".into()));
        }
        for p in &s.ball {
            check_range(
                file,
                "ball.x",
                p.x as f64,
                -half_len as f64,
                half_len as f64,
            )?;
            check_range(
                file,
                "ball.y",
                p.y as f64,
                -half_wid as f64,
                half_wid as f64,
            )?;
            check_range(file, "ball.z", p.z as f64, 0.0, 30.0)?;
        }
        if s.possession.is_empty() || s.possession[0].t != 0.0 {
            return Err(bad("владение должно быть задано с t = 0".into()));
        }
        for w in s.possession.windows(2) {
            if w[1].t <= w[0].t {
                return Err(bad("время смен владения должно строго возрастать".into()));
            }
        }
        if s.possession.iter().any(|p| p.team > 1) {
            return Err(bad("team должна быть 0 или 1".into()));
        }
        Ok(s)
    }
}
