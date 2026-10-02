use super::{check_order, check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "positioning.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseShape {
    /// Положение линии обороны от своей линии ворот при `line_height` 0 и 1.
    pub back_line_min_m: f32,
    pub back_line_max_m: f32,
    /// Положение переднего игрока блока при `block_height` 0 и 1.
    pub front_line_min_m: f32,
    pub front_line_max_m: f32,
    /// Предельное расстояние от линии обороны до переднего игрока при `compactness` 0 и 1.
    pub depth_loose_m: f32,
    pub depth_compact_m: f32,
    pub depth_floor_m: f32,
    /// Множитель ширины формации при `compactness` 0 и 1.
    pub width_scale_loose: f32,
    pub width_scale_compact: f32,
    /// Доля бокового положения мяча, на которую блок сдвигается к мячу.
    pub ball_lateral_k_loose: f32,
    pub ball_lateral_k_compact: f32,
    /// Линия следует за мячом вдоль поля: доля отклонения мяча от центра и предел сдвига.
    pub line_follow_k: f32,
    pub line_follow_max_m: f32,
    /// Линия обороны не стоит дальше от своих ворот, чем мяч минус этот запас: защитники не
    /// остаются впереди мяча, когда соперник прорвался за линию.
    pub line_behind_ball_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackShape {
    pub back_line_min_m: f32,
    pub back_line_max_m: f32,
    pub front_line_min_m: f32,
    pub front_line_max_m: f32,
    /// Множитель ширины формации при `width_in` 0 и 1.
    pub width_scale_min: f32,
    pub width_scale_max: f32,
    pub ball_lateral_k: f32,
    pub line_follow_k: f32,
    pub line_follow_max_m: f32,
    /// Нападающие держатся не дальше линии офсайда соперника плюс этот запас (когда включён).
    pub offside_margin_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeeperShape {
    pub base_m: f32,
    pub lateral_k: f32,
    pub max_lateral_m: f32,
    /// Насколько вратарь поднимается вместе с линией обороны.
    pub line_follow_k: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseTiming {
    pub transition_slow_s: f32,
    pub transition_fast_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineScatter {
    pub scatter_sd_m: f32,
    pub scatter_tau_s: f32,
    pub concentration_weight: f32,
    pub cohesion_weight: f32,
    pub familiarity_weight: f32,
    pub default_cohesion: f32,
    pub default_familiarity: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Steering {
    /// Ближе этого расстояния до якоря игрок идёт шагом, дальше `far_m` бежит на максимуме.
    pub near_m: f32,
    pub far_m: f32,
    /// Насколько `work_rate` сокращает `far_m`.
    pub work_rate_far_shrink: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositioningConfig {
    pub defense: DefenseShape,
    pub attack: AttackShape,
    pub keeper: KeeperShape,
    pub margin_m: f32,
    pub phase: PhaseTiming,
    pub line: LineScatter,
    pub steering: Steering,
}

impl PositioningConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: PositioningConfig = parse(FILE, json)?;
        let f = |n: &str, v: f32, lo: f64, hi: f64| check_range(FILE, n, v as f64, lo, hi);
        let o = |a: &str, b: &str, x: f32, y: f32| check_order(FILE, a, b, x as f64, y as f64);
        let d = &c.defense;
        f("defense.back_line_min_m", d.back_line_min_m, 5.0, 60.0)?;
        f("defense.back_line_max_m", d.back_line_max_m, 5.0, 60.0)?;
        o(
            "back_line_min_m",
            "back_line_max_m",
            d.back_line_min_m,
            d.back_line_max_m,
        )?;
        f("defense.front_line_min_m", d.front_line_min_m, 20.0, 100.0)?;
        f("defense.front_line_max_m", d.front_line_max_m, 20.0, 100.0)?;
        o(
            "front_line_min_m",
            "front_line_max_m",
            d.front_line_min_m,
            d.front_line_max_m,
        )?;
        f("defense.depth_loose_m", d.depth_loose_m, 10.0, 90.0)?;
        f("defense.depth_compact_m", d.depth_compact_m, 10.0, 90.0)?;
        o(
            "depth_compact_m",
            "depth_loose_m",
            d.depth_compact_m,
            d.depth_loose_m,
        )?;
        f("defense.depth_floor_m", d.depth_floor_m, 5.0, 60.0)?;
        o(
            "depth_floor_m",
            "depth_compact_m",
            d.depth_floor_m,
            d.depth_compact_m,
        )?;
        f("defense.width_scale_loose", d.width_scale_loose, 0.3, 1.5)?;
        f(
            "defense.width_scale_compact",
            d.width_scale_compact,
            0.3,
            1.5,
        )?;
        o(
            "width_scale_compact",
            "width_scale_loose",
            d.width_scale_compact,
            d.width_scale_loose,
        )?;
        f(
            "defense.ball_lateral_k_loose",
            d.ball_lateral_k_loose,
            0.0,
            1.0,
        )?;
        f(
            "defense.ball_lateral_k_compact",
            d.ball_lateral_k_compact,
            0.0,
            1.0,
        )?;
        o(
            "ball_lateral_k_loose",
            "ball_lateral_k_compact",
            d.ball_lateral_k_loose,
            d.ball_lateral_k_compact,
        )?;
        f("defense.line_follow_k", d.line_follow_k, 0.0, 1.0)?;
        f("defense.line_follow_max_m", d.line_follow_max_m, 0.0, 30.0)?;
        f(
            "defense.line_behind_ball_m",
            d.line_behind_ball_m,
            0.0,
            30.0,
        )?;

        let a = &c.attack;
        f("attack.back_line_min_m", a.back_line_min_m, 5.0, 80.0)?;
        f("attack.back_line_max_m", a.back_line_max_m, 5.0, 80.0)?;
        o(
            "back_line_min_m",
            "back_line_max_m",
            a.back_line_min_m,
            a.back_line_max_m,
        )?;
        f("attack.front_line_min_m", a.front_line_min_m, 30.0, 100.0)?;
        f("attack.front_line_max_m", a.front_line_max_m, 30.0, 100.0)?;
        o(
            "front_line_min_m",
            "front_line_max_m",
            a.front_line_min_m,
            a.front_line_max_m,
        )?;
        f("attack.width_scale_min", a.width_scale_min, 0.3, 1.5)?;
        f("attack.width_scale_max", a.width_scale_max, 0.3, 1.5)?;
        o(
            "width_scale_min",
            "width_scale_max",
            a.width_scale_min,
            a.width_scale_max,
        )?;
        f("attack.ball_lateral_k", a.ball_lateral_k, 0.0, 1.0)?;
        f("attack.line_follow_k", a.line_follow_k, 0.0, 1.0)?;
        f("attack.line_follow_max_m", a.line_follow_max_m, 0.0, 30.0)?;
        f("attack.offside_margin_m", a.offside_margin_m, 0.0, 10.0)?;

        let k = &c.keeper;
        f("keeper.base_m", k.base_m, 0.0, 10.0)?;
        f("keeper.lateral_k", k.lateral_k, 0.0, 1.0)?;
        f("keeper.max_lateral_m", k.max_lateral_m, 0.0, 6.0)?;
        f("keeper.line_follow_k", k.line_follow_k, 0.0, 1.0)?;

        f("margin_m", c.margin_m, 0.0, 10.0)?;
        f(
            "phase.transition_slow_s",
            c.phase.transition_slow_s,
            0.1,
            30.0,
        )?;
        f(
            "phase.transition_fast_s",
            c.phase.transition_fast_s,
            0.1,
            30.0,
        )?;
        o(
            "transition_fast_s",
            "transition_slow_s",
            c.phase.transition_fast_s,
            c.phase.transition_slow_s,
        )?;

        let l = &c.line;
        f("line.scatter_sd_m", l.scatter_sd_m, 0.0, 10.0)?;
        f("line.scatter_tau_s", l.scatter_tau_s, 0.1, 30.0)?;
        f(
            "line.concentration_weight",
            l.concentration_weight,
            0.0,
            1.0,
        )?;
        f("line.cohesion_weight", l.cohesion_weight, 0.0, 1.0)?;
        f("line.familiarity_weight", l.familiarity_weight, 0.0, 1.0)?;
        f("line.default_cohesion", l.default_cohesion, 0.0, 1.0)?;
        f("line.default_familiarity", l.default_familiarity, 0.0, 1.0)?;

        let s = &c.steering;
        f("steering.near_m", s.near_m, 0.0, 10.0)?;
        f("steering.far_m", s.far_m, 1.0, 50.0)?;
        o("near_m", "far_m", s.near_m, s.far_m)?;
        f(
            "steering.work_rate_far_shrink",
            s.work_rate_far_shrink,
            0.0,
            0.9,
        )?;
        Ok(c)
    }
}
