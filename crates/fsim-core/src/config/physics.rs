use super::{check_order, check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "physics.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Time {
    pub tick_hz: u32,
    pub half_minutes: u32,
    pub reaction_delay_min_s: f32,
    pub reaction_delay_max_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub max_speed_min_ms: f32,
    pub max_speed_max_ms: f32,
    pub acceleration_min_ms2: f32,
    pub acceleration_max_ms2: f32,
    pub deceleration_ms2: f32,
    pub lateral_accel_min_ms2: f32,
    pub lateral_accel_max_ms2: f32,
    pub ball_slowdown_min: f32,
    pub ball_slowdown_max: f32,
    pub gait_walk_fraction: f32,
    pub gait_jog_fraction: f32,
    pub radius_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ball {
    pub radius_m: f32,
    pub ground_pass_speed_min_ms: f32,
    pub ground_pass_speed_max_ms: f32,
    pub shot_speed_min_ms: f32,
    pub shot_speed_max_ms: f32,
    pub gravity_ms2: f32,
    pub air_drag_per_m: f32,
    pub rolling_decel_ms2: f32,
    pub bounce_restitution_min: f32,
    pub bounce_restitution_max: f32,
    pub bounce_tangential_keep: f32,
    pub bounce_min_vz_ms: f32,
    pub rain_bounce_scale: f32,
    pub rain_rolling_scale: f32,
    pub substeps: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pitch {
    pub length_m: f32,
    pub width_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub width_m: f32,
    pub height_m: f32,
    pub depth_m: f32,
    pub post_radius_m: f32,
    pub post_restitution: f32,
    pub net_restitution: f32,
    pub net_tangential_keep: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsConfig {
    pub time: Time,
    pub player: Player,
    pub ball: Ball,
    pub pitch: Pitch,
    pub goal: Goal,
}

impl PhysicsConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: PhysicsConfig = parse(FILE, json)?;
        c.validate()?;
        Ok(c)
    }

    /// Шаг физики, с.
    pub fn dt(&self) -> f32 {
        1.0 / self.time.tick_hz as f32
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let f = |name: &str, v: f32, lo: f64, hi: f64| check_range(FILE, name, v as f64, lo, hi);
        let ord =
            |lo: &str, hi: &str, a: f32, b: f32| check_order(FILE, lo, hi, a as f64, b as f64);
        let (t, p, b, g, goal) = (
            &self.time,
            &self.player,
            &self.ball,
            &self.pitch,
            &self.goal,
        );

        check_range(FILE, "time.tick_hz", t.tick_hz as f64, 1.0, 240.0)?;
        check_range(FILE, "time.half_minutes", t.half_minutes as f64, 1.0, 90.0)?;
        f(
            "time.reaction_delay_min_s",
            t.reaction_delay_min_s,
            0.0,
            2.0,
        )?;
        f(
            "time.reaction_delay_max_s",
            t.reaction_delay_max_s,
            0.0,
            2.0,
        )?;
        ord(
            "reaction_delay_min_s",
            "reaction_delay_max_s",
            t.reaction_delay_min_s,
            t.reaction_delay_max_s,
        )?;

        f("player.max_speed_min_ms", p.max_speed_min_ms, 3.0, 12.0)?;
        f("player.max_speed_max_ms", p.max_speed_max_ms, 3.0, 12.0)?;
        ord(
            "max_speed_min_ms",
            "max_speed_max_ms",
            p.max_speed_min_ms,
            p.max_speed_max_ms,
        )?;
        f(
            "player.acceleration_min_ms2",
            p.acceleration_min_ms2,
            1.0,
            10.0,
        )?;
        f(
            "player.acceleration_max_ms2",
            p.acceleration_max_ms2,
            1.0,
            10.0,
        )?;
        ord(
            "acceleration_min_ms2",
            "acceleration_max_ms2",
            p.acceleration_min_ms2,
            p.acceleration_max_ms2,
        )?;
        f("player.deceleration_ms2", p.deceleration_ms2, 1.0, 15.0)?;
        f(
            "player.lateral_accel_min_ms2",
            p.lateral_accel_min_ms2,
            1.0,
            15.0,
        )?;
        f(
            "player.lateral_accel_max_ms2",
            p.lateral_accel_max_ms2,
            1.0,
            15.0,
        )?;
        ord(
            "lateral_accel_min_ms2",
            "lateral_accel_max_ms2",
            p.lateral_accel_min_ms2,
            p.lateral_accel_max_ms2,
        )?;
        f("player.ball_slowdown_min", p.ball_slowdown_min, 0.0, 0.5)?;
        f("player.ball_slowdown_max", p.ball_slowdown_max, 0.0, 0.5)?;
        ord(
            "ball_slowdown_min",
            "ball_slowdown_max",
            p.ball_slowdown_min,
            p.ball_slowdown_max,
        )?;
        f("player.gait_walk_fraction", p.gait_walk_fraction, 0.05, 1.0)?;
        f("player.gait_jog_fraction", p.gait_jog_fraction, 0.05, 1.0)?;
        ord(
            "gait_walk_fraction",
            "gait_jog_fraction",
            p.gait_walk_fraction,
            p.gait_jog_fraction,
        )?;
        f("player.radius_m", p.radius_m, 0.1, 1.5)?;

        f("ball.radius_m", b.radius_m, 0.05, 0.2)?;
        f(
            "ball.ground_pass_speed_min_ms",
            b.ground_pass_speed_min_ms,
            1.0,
            40.0,
        )?;
        f(
            "ball.ground_pass_speed_max_ms",
            b.ground_pass_speed_max_ms,
            1.0,
            40.0,
        )?;
        ord(
            "ground_pass_speed_min_ms",
            "ground_pass_speed_max_ms",
            b.ground_pass_speed_min_ms,
            b.ground_pass_speed_max_ms,
        )?;
        f("ball.shot_speed_min_ms", b.shot_speed_min_ms, 1.0, 45.0)?;
        f("ball.shot_speed_max_ms", b.shot_speed_max_ms, 1.0, 45.0)?;
        ord(
            "shot_speed_min_ms",
            "shot_speed_max_ms",
            b.shot_speed_min_ms,
            b.shot_speed_max_ms,
        )?;
        f("ball.gravity_ms2", b.gravity_ms2, 9.0, 10.0)?;
        f("ball.air_drag_per_m", b.air_drag_per_m, 0.0, 0.1)?;
        f("ball.rolling_decel_ms2", b.rolling_decel_ms2, 0.0, 5.0)?;
        f(
            "ball.bounce_restitution_min",
            b.bounce_restitution_min,
            0.0,
            1.0,
        )?;
        f(
            "ball.bounce_restitution_max",
            b.bounce_restitution_max,
            0.0,
            1.0,
        )?;
        ord(
            "bounce_restitution_min",
            "bounce_restitution_max",
            b.bounce_restitution_min,
            b.bounce_restitution_max,
        )?;
        f(
            "ball.bounce_tangential_keep",
            b.bounce_tangential_keep,
            0.0,
            1.0,
        )?;
        f("ball.bounce_min_vz_ms", b.bounce_min_vz_ms, 0.0, 5.0)?;
        f("ball.rain_bounce_scale", b.rain_bounce_scale, 0.1, 1.0)?;
        f("ball.rain_rolling_scale", b.rain_rolling_scale, 0.1, 1.0)?;
        check_range(FILE, "ball.substeps", b.substeps as f64, 1.0, 64.0)?;

        f("pitch.length_m", g.length_m, 90.0, 120.0)?;
        f("pitch.width_m", g.width_m, 45.0, 90.0)?;

        f("goal.width_m", goal.width_m, 5.0, 10.0)?;
        f("goal.height_m", goal.height_m, 1.5, 3.5)?;
        f("goal.depth_m", goal.depth_m, 0.5, 4.0)?;
        f("goal.post_radius_m", goal.post_radius_m, 0.01, 0.2)?;
        f("goal.post_restitution", goal.post_restitution, 0.0, 1.0)?;
        f("goal.net_restitution", goal.net_restitution, 0.0, 1.0)?;
        f(
            "goal.net_tangential_keep",
            goal.net_tangential_keep,
            0.0,
            1.0,
        )?;
        // Ворота должны помещаться в ширину поля.
        if goal.width_m >= g.width_m {
            return Err(ConfigError::Invalid {
                file: FILE.into(),
                msg: "ширина ворот не меньше ширины поля".into(),
            });
        }
        Ok(())
    }
}
