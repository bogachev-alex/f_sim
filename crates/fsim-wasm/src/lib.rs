//! Обёртка ядра для браузера. Движок работает в Web Worker и отдаёт кадры плоским
//! `Float32Array`. Раскладка кадра описана в `FrameLayout` и уходит в TypeScript вместе с
//! остальными типами через `ts-rs` (`just gen-types`).

mod info;
mod sink;

pub use info::{
    Catalog, FrameLayout, InfoParams, MatchInfo, PitchInfo, PlayerInfo, ScenarioEntry, StyleEntry,
};
pub use sink::RecordingSink;

use fsim_core::attrs::Attrs;
use fsim_core::config::Config;
use fsim_core::curves::CurveSet;
use fsim_core::data::Squad;
use fsim_core::embedded;
use fsim_core::live_match::{run_match, LiveMatch};
use fsim_core::match_loop::{run_sandbox, MatchSim};
use fsim_core::physics::body::BodyParams;
use fsim_core::positioning::team::TeamSetup;
use fsim_core::scenario_sim::{run_scenario, ScenarioSim};
use fsim_core::world::PLAYERS;
use fsim_gen::{generate_equal_pair, generate_squad, GenConfig, SquadParams};
use glam::Vec2;
use wasm_bindgen::prelude::*;

const GENERATOR: &str = include_str!("../../../config/generator.json");

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Хэш песочницы; должен совпадать с `fsim determinism` в нативной сборке.
#[wasm_bindgen]
pub fn determinism_hash(seed: u64, ticks: u32) -> Result<String, JsError> {
    let cfg = embedded::config().map_err(err)?;
    Ok(format!("{:016x}", run_sandbox(seed, ticks, &cfg)))
}

/// Хэш матча; должен совпадать с `fsim determinism --match`.
#[wasm_bindgen]
pub fn match_hash(seed: u64, ticks: u32, style_a: &str, style_b: &str) -> Result<String, JsError> {
    let cfg = embedded::config().map_err(err)?;
    Ok(format!(
        "{:016x}",
        run_match(seed, ticks, &cfg, [style_a, style_b], ["4-3-3", "4-3-3"]).map_err(err)?
    ))
}

/// Хэш сценария; должен совпадать с `fsim determinism --scenario`.
#[wasm_bindgen]
pub fn scenario_hash(seed: u64, ticks: u32, name: &str) -> Result<String, JsError> {
    let cfg = embedded::config().map_err(err)?;
    Ok(format!(
        "{:016x}",
        run_scenario(seed, ticks, &cfg, name).map_err(err)?
    ))
}

/// Схемы, стили и сценарии (JSON, тип `Catalog`).
#[wasm_bindgen]
pub fn catalog() -> Result<String, JsError> {
    let cfg = embedded::config().map_err(err)?;
    serde_json::to_string(&Catalog::build(&cfg)).map_err(err)
}

enum Inner {
    Sandbox(Box<MatchSim<RecordingSink>>),
    Scenario(Box<ScenarioSim<RecordingSink>>, u32),
    Live(Box<LiveMatch<RecordingSink>>),
}

/// Движок матча для визуализатора.
#[wasm_bindgen]
pub struct Engine {
    inner: Inner,
    info_json: String,
}

/// Две команды из одного распределения; при разных схемах вторая генерируется отдельно.
fn make_squads(
    cfg: &Config,
    seed: u64,
    quality: f32,
    form_a: &str,
    form_b: &str,
) -> Result<(Squad, Squad), JsError> {
    let gen = GenConfig::from_json(GENERATOR).map_err(err)?;
    let (a, mut b) =
        generate_equal_pair(&gen, &cfg.formations, seed, form_a, Some(quality)).map_err(err)?;
    if form_a != form_b {
        let mut params = SquadParams::new(seed ^ 0x9E37_79B9, "B", form_b);
        params.quality = Some(quality);
        b = generate_squad(&gen, &cfg.formations, &params).map_err(err)?;
    }
    Ok((a, b))
}

fn bodies(cfg: &Config, a: &Squad, b: &Squad) -> Result<[BodyParams; PLAYERS], JsError> {
    let curves = CurveSet::build(&cfg.curves).map_err(err)?;
    let mut body = [BodyParams {
        max_speed: 0.0,
        accel: 0.0,
        decel: 0.0,
        lat_accel: 0.0,
    }; PLAYERS];
    for (i, p) in a
        .players
        .iter()
        .take(11)
        .chain(b.players.iter().take(11))
        .enumerate()
    {
        let attrs: Attrs = p.attrs_array();
        body[i] = BodyParams::from_attrs(&attrs, &curves, &cfg.physics);
    }
    Ok(body)
}

#[wasm_bindgen]
impl Engine {
    /// Песочница физики: игроки бегут к случайным точкам, мяч бьют (`docs/09`, M1).
    #[wasm_bindgen(constructor)]
    pub fn new(
        seed: u64,
        quality: f32,
        formation_a: &str,
        formation_b: &str,
    ) -> Result<Engine, JsError> {
        let cfg = embedded::config().map_err(err)?;
        let (mut a, mut b) = make_squads(&cfg, seed, quality, formation_a, formation_b)?;
        a.name = "A".into();
        b.name = "B".into();
        let body = bodies(&cfg, &a, &b)?;
        let mut sim = MatchSim::new(seed, &cfg.physics, &cfg.sandbox, body, RecordingSink::new());
        let (l, w) = (cfg.physics.pitch.length_m, cfg.physics.pitch.width_m);
        let slots = |s: &Squad| cfg.formations.0[&s.formation].clone();
        let mut starts = [Vec2::ZERO; PLAYERS];
        for (i, s) in slots(&a).iter().enumerate() {
            starts[i] = Vec2::new((s.x - 0.5) * l, (s.y - 0.5) * w);
        }
        for (i, s) in slots(&b).iter().enumerate() {
            starts[11 + i] = Vec2::new((0.5 - s.x) * l, (0.5 - s.y) * w);
        }
        sim.set_player_positions(&starts);
        let roles = [
            a.players.iter().take(11).map(|p| p.role.clone()).collect(),
            b.players.iter().take(11).map(|p| p.role.clone()).collect(),
        ];
        let info = MatchInfo::build(
            &cfg,
            &InfoParams {
                mode: "sandbox",
                seed,
                scenario: None,
                duration_ticks: 0,
                styles: ["—".into(), "—".into()],
                squads: [&a, &b],
                roles,
            },
            &body,
        );
        let info_json = serde_json::to_string(&info).map_err(err)?;
        // Нулевой кадр: начальное состояние.
        let mut sink = RecordingSink::new();
        {
            use fsim_core::sink::FrameSink;
            sink.frame(sim.world.tick, &sim.world);
        }
        *sim.sink_mut() = sink;
        Ok(Engine {
            inner: Inner::Sandbox(Box::new(sim)),
            info_json,
        })
    }

    /// Сценарий: мяч и владение по скрипту, игроки держат форму. Пустые строки значат
    /// «как в сценарии».
    pub fn scenario(
        seed: u64,
        scenario_id: &str,
        style_a: &str,
        style_b: &str,
        formation_a: &str,
        formation_b: &str,
        quality: f32,
    ) -> Result<Engine, JsError> {
        let cfg = embedded::config().map_err(err)?;
        let sc = cfg
            .scenarios
            .get(scenario_id)
            .ok_or_else(|| err(format!("сценарий `{scenario_id}` не найден")))?;
        let pick = |given: &str, default: &str| {
            if given.is_empty() {
                default.to_string()
            } else {
                given.to_string()
            }
        };
        let styles = [pick(style_a, &sc.styles[0]), pick(style_b, &sc.styles[1])];
        let forms = [
            pick(formation_a, &sc.formations[0]),
            pick(formation_b, &sc.formations[1]),
        ];
        let (mut a, mut b) = make_squads(&cfg, seed, quality, &forms[0], &forms[1])?;
        a.name = "A".into();
        b.name = "B".into();
        let mut setups = Vec::with_capacity(2);
        for (k, squad) in [&a, &b].into_iter().enumerate() {
            let style = cfg
                .styles
                .get(&styles[k])
                .ok_or_else(|| err(format!("стиль `{}` не найден", styles[k])))?;
            setups.push(TeamSetup::from_squad(&cfg, squad, style).map_err(err)?);
        }
        let [ta, tb]: [TeamSetup; 2] = setups.try_into().map_err(|_| err("две команды"))?;
        let roles = [
            ta.slots.iter().map(|s| s.role.clone()).collect::<Vec<_>>(),
            tb.slots.iter().map(|s| s.role.clone()).collect::<Vec<_>>(),
        ];
        let body = bodies(&cfg, &a, &b)?;
        let duration_ticks = (sc.duration_s * cfg.physics.time.tick_hz as f32) as u32;
        let info = MatchInfo::build(
            &cfg,
            &InfoParams {
                mode: "scenario",
                seed,
                scenario: Some(scenario_id),
                duration_ticks,
                styles: styles.clone(),
                squads: [&a, &b],
                roles,
            },
            &body,
        );
        let info_json = serde_json::to_string(&info).map_err(err)?;
        let sim = ScenarioSim::new(seed, &cfg, sc, [ta, tb], RecordingSink::new());
        Ok(Engine {
            inner: Inner::Scenario(Box::new(sim), duration_ticks),
            info_json,
        })
    }

    /// Матч: мяч по физике, решения владельца, передачи, отбор, удары. Пустые стили и схемы
    /// значат «стиль по умолчанию схемы»: `positional` против `gegenpress`, 4-3-3.
    pub fn live(
        seed: u64,
        quality: f32,
        style_a: &str,
        style_b: &str,
        formation_a: &str,
        formation_b: &str,
    ) -> Result<Engine, JsError> {
        let cfg = embedded::config().map_err(err)?;
        let pick = |given: &str, default: &str| {
            if given.is_empty() {
                default.to_string()
            } else {
                given.to_string()
            }
        };
        let styles = [pick(style_a, "positional"), pick(style_b, "gegenpress")];
        let forms = [pick(formation_a, "4-3-3"), pick(formation_b, "4-3-3")];
        let (mut a, mut b) = make_squads(&cfg, seed, quality, &forms[0], &forms[1])?;
        a.name = "A".into();
        b.name = "B".into();
        let mut setups = Vec::with_capacity(2);
        for (k, squad) in [&a, &b].into_iter().enumerate() {
            let style = cfg
                .styles
                .get(&styles[k])
                .ok_or_else(|| err(format!("стиль `{}` не найден", styles[k])))?;
            setups.push(TeamSetup::from_squad(&cfg, squad, style).map_err(err)?);
        }
        let [ta, tb]: [TeamSetup; 2] = setups.try_into().map_err(|_| err("две команды"))?;
        let roles = [
            ta.slots.iter().map(|s| s.role.clone()).collect::<Vec<_>>(),
            tb.slots.iter().map(|s| s.role.clone()).collect::<Vec<_>>(),
        ];
        let body = bodies(&cfg, &a, &b)?;
        let info = MatchInfo::build(
            &cfg,
            &InfoParams {
                mode: "match",
                seed,
                scenario: None,
                duration_ticks: 0,
                styles: styles.clone(),
                squads: [&a, &b],
                roles,
            },
            &body,
        );
        let info_json = serde_json::to_string(&info).map_err(err)?;
        let mut m = LiveMatch::new(seed, &cfg, [ta, tb], RecordingSink::new());
        m.control_grid_enabled = false;
        Ok(Engine {
            inner: Inner::Live(Box::new(m)),
            info_json,
        })
    }

    /// Статистика матча (JSON) или `null` вне режима матча.
    pub fn stats(&self) -> String {
        match &self.inner {
            Inner::Live(m) => {
                let score = m.stats.goals;
                let mut v = serde_json::to_value(m.stats).unwrap_or(serde_json::Value::Null);
                v["score"] = serde_json::json!(score);
                v.to_string()
            }
            _ => "null".to_string(),
        }
    }

    /// Описание матча (JSON, тип `MatchInfo`).
    pub fn info(&self) -> String {
        self.info_json.clone()
    }

    /// Текущий тик.
    pub fn tick(&self) -> u32 {
        match &self.inner {
            Inner::Sandbox(s) => s.world.tick,
            Inner::Scenario(s, _) => s.world.tick,
            Inner::Live(m) => m.world.tick,
        }
    }

    /// Считает до тика `target` включительно и возвращает записанные кадры, подряд, по
    /// `FrameLayout::stride` чисел на кадр. Нулевой кадр отдаётся при первом вызове;
    /// сценарий не идёт дальше своей длительности.
    pub fn run_to(&mut self, target: u32) -> Vec<f32> {
        match &mut self.inner {
            Inner::Sandbox(sim) => {
                while sim.world.tick < target {
                    sim.step();
                }
                sim.sink_mut().take()
            }
            Inner::Live(m) => {
                while m.world.tick < target {
                    m.step();
                }
                m.sink_mut().take()
            }
            Inner::Scenario(sim, duration) => {
                let target = target.min(*duration);
                while sim.world.tick < target {
                    sim.step();
                }
                sim.sink_mut().take()
            }
        }
    }
}
