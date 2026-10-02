//! Конфиги, вшитые в сборку. Нужны WASM, где файловой системы нет, и тестам.

use crate::config::{Config, ConfigError, ConfigSource};

macro_rules! files {
    ($($path:literal),* $(,)?) => {
        const FILES: &[(&str, &str)] = &[
            $(($path, include_str!(concat!("../../../config/", $path)))),*
        ];
    };
}

files!(
    "attributes.json",
    "composites.json",
    "physics.json",
    "response_curves.json",
    "formations.json",
    "perf.json",
    "sandbox.json",
    "positioning.json",
    "defense.json",
    "decision.json",
    "xt_grid.json",
    "styles/balanced.json",
    "styles/direct_long_ball.json",
    "styles/gegenpress.json",
    "styles/low_block_counter.json",
    "styles/positional.json",
    "styles/wing_play.json",
    "roles/anchor_dm.json",
    "roles/attacking_mid.json",
    "roles/ball_playing_cb.json",
    "roles/box_to_box.json",
    "roles/classic_keeper.json",
    "roles/classic_winger.json",
    "roles/deep_lying_forward.json",
    "roles/deep_playmaker.json",
    "roles/defensive_fb.json",
    "roles/false_nine.json",
    "roles/inverted_fb.json",
    "roles/inverted_winger.json",
    "roles/line_striker.json",
    "roles/mezzala.json",
    "roles/overlapping_fb.json",
    "roles/pressing_forward.json",
    "roles/shadow_striker.json",
    "roles/stopper_cb.json",
    "roles/sweeper_keeper.json",
    "roles/target_man.json",
    "roles/wide_playmaker.json",
    "scenarios/ball_sweep.json",
    "scenarios/back_pass_trigger.json",
    "scenarios/buildup_vs_press.json",
    "scenarios/counter_press_loss.json",
    "scenarios/possession_flips.json",
    "scenarios/shape_hold.json",
    "scenarios/switch_play.json",
);

struct Embedded;

impl ConfigSource for Embedded {
    fn read(&self, path: &str) -> Result<String, ConfigError> {
        FILES
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, s)| (*s).to_owned())
            .ok_or_else(|| ConfigError::Invalid {
                file: path.into(),
                msg: "файл не вшит в сборку".into(),
            })
    }

    fn list(&self, dir: &str) -> Result<Vec<String>, ConfigError> {
        let prefix = format!("{dir}/");
        let mut names: Vec<String> = FILES
            .iter()
            .filter_map(|(p, _)| p.strip_prefix(&prefix))
            .filter_map(|p| p.strip_suffix(".json"))
            .map(str::to_owned)
            .collect();
        names.sort();
        Ok(names)
    }
}

pub fn config() -> Result<Config, ConfigError> {
    Config::load_from(&Embedded)
}
