use super::{check_order, check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "decision.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    /// Задержка перед решением после получения мяча: от `max_s` при слабом `awareness` до `min_s`.
    pub min_s: f32,
    pub max_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidates {
    /// Сколько кандидатов остаётся после эвристики (этап 1) и после точной оценки (этап 2).
    pub stage1_keep: u32,
    pub stage2_keep: u32,
    /// Из `stage1_keep` столько мест резервируется под безопасные пасы в ноги и ведение, чтобы
    /// быстрая оценка по приросту xT не вытесняла их рискованными пасами вперёд.
    pub stage1_reserve: u32,
    pub dribble_dirs: u32,
    pub dribble_min_m: f32,
    pub dribble_max_m: f32,
    pub dribble_spread_deg: f32,
    /// Пасы в пространство: точек на партнёра и шаг вперёд от его позиции.
    pub space_points: u32,
    pub space_step_m: f32,
    /// Пас за спину защитникам: насколько глубже линии защитников соперника.
    pub through_depth_m: f32,
    pub long_pass_min_m: f32,
    /// Вынос: дальность и зона у своих ворот, в которой он рассматривается.
    pub clearance_distance_m: f32,
    pub clearance_zone_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassModel {
    pub speed_min_ms: f32,
    pub speed_max_ms: f32,
    /// Скорость мяча у получателя, к которой стремится расчёт силы паса.
    pub receive_speed_ms: f32,
    pub loft_deg: f32,
    /// Точек траектории для проверки перехвата.
    pub lane_points: u32,
    pub intercept_softness_s: f32,
    pub receiver_softness_s: f32,
    /// Радиус, в котором получатель принимает неточный пас.
    pub catch_radius_m: f32,
    pub angle_sigma_best_deg: f32,
    pub angle_sigma_worst_deg: f32,
    pub speed_sigma_best: f32,
    pub speed_sigma_worst: f32,
    /// Рост ошибки с давлением (умножается на `1 − press_resist`).
    pub pressure_gain: f32,
    /// Задержка реакции соперников при оценке перехвата.
    pub defender_react_s: f32,
    /// Поправка вероятности успеха верхового паса: борьба за мяч в воздухе, отскок и качение
    /// после приземления в оценке не моделируются; значение по итогам калибровки на матчах.
    pub loft_success_scale: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pressure {
    pub radius_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Utility {
    pub risk_weight: f32,
    pub loss_base: f32,
    pub patience_risk_gain: f32,
    pub ahead_gain: f32,
    pub short_gain: f32,
    pub direct_gain: f32,
    pub long_gain: f32,
    pub width_gain: f32,
    pub tempo_gain: f32,
    pub dribble_gain: f32,
    pub carry_cost_m: f32,
    pub shield_value: f32,
    /// Пасы с вероятностью успеха ниже порога почти не выбираются: порог `min_pass_success −
    /// min_pass_success_directness·(directness − 0,5)` ниже у прямолинейных стилей; штраф
    /// `low_success_penalty` вычитается из полезности такого паса.
    pub min_pass_success: f32,
    pub min_pass_success_directness: f32,
    pub low_success_penalty: f32,
    /// Штраф удержания мяча: растёт с временем владения, вытесняет бесконечное ведение.
    pub hold_cost_per_s: f32,
    pub lane_clear_ref_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Softmax {
    pub base: f32,
    pub k_decisions: f32,
    pub k_morale: f32,
    pub k_fatigue: f32,
    pub k_pressure: f32,
    /// Мораль и усталость до M6 постоянны.
    pub morale: f32,
    pub fatigue: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub margin: f32,
    pub time_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub reach_m: f32,
    pub reach_height_m: f32,
    /// Логит приёма: `base + skill_gain·control − speed_cost·v_отн − pressure_cost·давление`.
    pub base: f32,
    pub skill_gain: f32,
    pub speed_cost: f32,
    pub pressure_cost: f32,
    /// Прибавка к логиту у соперника, перехватывающего мяч.
    pub intercept_bonus: f32,
    pub lockout_s: f32,
    pub deflect_speed_ms: f32,
    pub carry_offset_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Duel {
    pub range_m: f32,
    pub attempt_rate_hz: f32,
    pub aggression_gain: f32,
    pub skill_gain: f32,
    pub speed_gain: f32,
    pub win_possession_share: f32,
    pub stumble_s: f32,
    pub cooldown_s: f32,
    pub push_ms: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restart {
    pub dead_s: f32,
    pub goal_dead_s: f32,
    pub arrive_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Xg {
    /// Логит xG: `a − b·ln(d) + c·ln(угол/угол с 11 м) − pressure·давление + skill_gain·(finish − 0,5)`.
    pub a: f32,
    pub b_ln_dist: f32,
    pub c_angle: f32,
    pub pressure: f32,
    pub skill_gain: f32,
    pub min_dist_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    /// Досягаемость вратаря (по земле и по высоте).
    pub reach_m: f32,
    pub reach_height_m: f32,
    /// Логит сейва: `base + gain·gk_shot_stop − speed_cost·скорость удара`.
    pub base: f32,
    pub gain: f32,
    pub speed_cost: f32,
    /// Доля сейвов, закончившихся ловлей; остальные отбиты.
    pub catch_share: f32,
    pub parry_speed_ms: f32,
    pub lockout_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shot {
    pub max_distance_m: f32,
    pub xg: Xg,
    /// Ценность гола в единицах xT и цена промаха (потеря мяча без ценной позиции у соперника).
    pub goal_value: f32,
    pub miss_cost: f32,
    /// Прицел: отступ от штанги, высота и доля ударов в дальний от вратаря угол.
    pub aim_inset_m: f32,
    pub aim_height_m: f32,
    pub far_corner_share: f32,
    pub speed_min_ms: f32,
    pub speed_max_ms: f32,
    pub angle_sigma_best_deg: f32,
    pub angle_sigma_worst_deg: f32,
    pub pressure_gain: f32,
    /// Бонус стиля за дальние удары: `long_shot_gain × (вес long_shots − 0,15)` при дистанции от 20 м.
    pub long_shot_gain: f32,
    pub save: Save,
}

/// Решения владельца мяча и исполнение (`docs/03-decisions.md`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionConfig {
    pub period_s: f32,
    pub reaction: Reaction,
    pub candidates: Candidates,
    pub pass: PassModel,
    pub pressure: Pressure,
    pub utility: Utility,
    pub softmax: Softmax,
    pub commit: Commit,
    pub control: Control,
    pub duel: Duel,
    pub restart: Restart,
    pub shot: Shot,
}

impl DecisionConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: DecisionConfig = parse(FILE, json)?;
        let f = |n: &str, v: f32, lo: f64, hi: f64| check_range(FILE, n, v as f64, lo, hi);
        let o = |a: &str, b: &str, x: f32, y: f32| check_order(FILE, a, b, x as f64, y as f64);
        f("period_s", c.period_s, 0.05, 2.0)?;
        f("reaction.min_s", c.reaction.min_s, 0.0, 2.0)?;
        f("reaction.max_s", c.reaction.max_s, 0.0, 2.0)?;
        o(
            "reaction.min_s",
            "reaction.max_s",
            c.reaction.min_s,
            c.reaction.max_s,
        )?;
        let k = &c.candidates;
        check_range(
            FILE,
            "candidates.stage1_keep",
            k.stage1_keep as f64,
            1.0,
            40.0,
        )?;
        check_range(
            FILE,
            "candidates.stage2_keep",
            k.stage2_keep as f64,
            1.0,
            12.0,
        )?;
        o(
            "stage2_keep",
            "stage1_keep",
            k.stage2_keep as f32,
            k.stage1_keep as f32,
        )?;
        check_range(
            FILE,
            "candidates.dribble_dirs",
            k.dribble_dirs as f64,
            1.0,
            9.0,
        )?;
        f("candidates.dribble_min_m", k.dribble_min_m, 0.5, 20.0)?;
        f("candidates.dribble_max_m", k.dribble_max_m, 0.5, 30.0)?;
        o(
            "dribble_min_m",
            "dribble_max_m",
            k.dribble_min_m,
            k.dribble_max_m,
        )?;
        f(
            "candidates.dribble_spread_deg",
            k.dribble_spread_deg,
            0.0,
            180.0,
        )?;
        check_range(
            FILE,
            "candidates.space_points",
            k.space_points as f64,
            0.0,
            4.0,
        )?;
        f("candidates.space_step_m", k.space_step_m, 1.0, 30.0)?;
        f("candidates.through_depth_m", k.through_depth_m, 0.0, 30.0)?;
        f("candidates.long_pass_min_m", k.long_pass_min_m, 10.0, 80.0)?;
        f(
            "candidates.clearance_distance_m",
            k.clearance_distance_m,
            10.0,
            100.0,
        )?;
        f("candidates.clearance_zone_m", k.clearance_zone_m, 5.0, 60.0)?;
        let p = &c.pass;
        f("pass.speed_min_ms", p.speed_min_ms, 5.0, 40.0)?;
        f("pass.speed_max_ms", p.speed_max_ms, 5.0, 40.0)?;
        o(
            "speed_min_ms",
            "speed_max_ms",
            p.speed_min_ms,
            p.speed_max_ms,
        )?;
        f("pass.receive_speed_ms", p.receive_speed_ms, 0.5, 20.0)?;
        f("pass.loft_deg", p.loft_deg, 5.0, 60.0)?;
        check_range(FILE, "pass.lane_points", p.lane_points as f64, 3.0, 32.0)?;
        f(
            "pass.intercept_softness_s",
            p.intercept_softness_s,
            0.01,
            2.0,
        )?;
        f("pass.receiver_softness_s", p.receiver_softness_s, 0.01, 2.0)?;
        f("pass.catch_radius_m", p.catch_radius_m, 0.3, 4.0)?;
        f(
            "pass.angle_sigma_best_deg",
            p.angle_sigma_best_deg,
            0.0,
            30.0,
        )?;
        f(
            "pass.angle_sigma_worst_deg",
            p.angle_sigma_worst_deg,
            0.0,
            45.0,
        )?;
        o(
            "angle_sigma_best_deg",
            "angle_sigma_worst_deg",
            p.angle_sigma_best_deg,
            p.angle_sigma_worst_deg,
        )?;
        f("pass.speed_sigma_best", p.speed_sigma_best, 0.0, 0.5)?;
        f("pass.speed_sigma_worst", p.speed_sigma_worst, 0.0, 0.5)?;
        o(
            "speed_sigma_best",
            "speed_sigma_worst",
            p.speed_sigma_best,
            p.speed_sigma_worst,
        )?;
        f("pass.pressure_gain", p.pressure_gain, 0.0, 5.0)?;
        f("pass.defender_react_s", p.defender_react_s, 0.0, 2.0)?;
        f("pass.loft_success_scale", p.loft_success_scale, 0.1, 1.0)?;
        f("pressure.radius_m", c.pressure.radius_m, 1.0, 20.0)?;
        let u = &c.utility;
        f("utility.risk_weight", u.risk_weight, 0.0, 5.0)?;
        f("utility.loss_base", u.loss_base, 0.0, 0.1)?;
        f("utility.patience_risk_gain", u.patience_risk_gain, 0.0, 2.0)?;
        f("utility.ahead_gain", u.ahead_gain, 0.0, 1.0)?;
        for (n, v) in [
            ("short_gain", u.short_gain),
            ("direct_gain", u.direct_gain),
            ("long_gain", u.long_gain),
            ("width_gain", u.width_gain),
            ("tempo_gain", u.tempo_gain),
            ("dribble_gain", u.dribble_gain),
            ("shield_value", u.shield_value),
        ] {
            f(&format!("utility.{n}"), v, -0.1, 0.1)?;
        }
        f("utility.carry_cost_m", u.carry_cost_m, 0.0, 0.01)?;
        f("utility.hold_cost_per_s", u.hold_cost_per_s, 0.0, 0.1)?;
        f("utility.min_pass_success", u.min_pass_success, 0.0, 1.0)?;
        f(
            "utility.min_pass_success_directness",
            u.min_pass_success_directness,
            0.0,
            1.0,
        )?;
        f(
            "utility.low_success_penalty",
            u.low_success_penalty,
            0.0,
            1.0,
        )?;
        f("utility.lane_clear_ref_m", u.lane_clear_ref_m, 0.5, 20.0)?;
        let s = &c.softmax;
        f("softmax.base", s.base, 0.0005, 1.0)?;
        for (n, v) in [
            ("k_decisions", s.k_decisions),
            ("k_morale", s.k_morale),
            ("k_fatigue", s.k_fatigue),
            ("k_pressure", s.k_pressure),
        ] {
            f(&format!("softmax.{n}"), v, 0.0, 1.0)?;
        }
        f("softmax.morale", s.morale, 0.0, 1.0)?;
        f("softmax.fatigue", s.fatigue, 0.0, 1.0)?;
        f("commit.margin", c.commit.margin, 0.0, 0.1)?;
        f("commit.time_s", c.commit.time_s, 0.0, 5.0)?;
        let ct = &c.control;
        f("control.reach_m", ct.reach_m, 0.3, 3.0)?;
        f("control.reach_height_m", ct.reach_height_m, 0.3, 3.0)?;
        f("control.base", ct.base, -5.0, 10.0)?;
        f("control.skill_gain", ct.skill_gain, 0.0, 20.0)?;
        f("control.speed_cost", ct.speed_cost, 0.0, 3.0)?;
        f("control.pressure_cost", ct.pressure_cost, 0.0, 10.0)?;
        f("control.intercept_bonus", ct.intercept_bonus, -5.0, 5.0)?;
        f("control.lockout_s", ct.lockout_s, 0.0, 3.0)?;
        f("control.deflect_speed_ms", ct.deflect_speed_ms, 0.0, 15.0)?;
        f("control.carry_offset_m", ct.carry_offset_m, 0.2, 2.0)?;
        let d = &c.duel;
        f("duel.range_m", d.range_m, 0.3, 3.0)?;
        f("duel.attempt_rate_hz", d.attempt_rate_hz, 0.0, 10.0)?;
        f("duel.aggression_gain", d.aggression_gain, 0.0, 3.0)?;
        f("duel.skill_gain", d.skill_gain, 0.0, 20.0)?;
        f("duel.speed_gain", d.speed_gain, 0.0, 2.0)?;
        f(
            "duel.win_possession_share",
            d.win_possession_share,
            0.0,
            1.0,
        )?;
        f("duel.stumble_s", d.stumble_s, 0.0, 5.0)?;
        f("duel.cooldown_s", d.cooldown_s, 0.0, 10.0)?;
        f("duel.push_ms", d.push_ms, 0.0, 15.0)?;
        let sh = &c.shot;
        f("shot.max_distance_m", sh.max_distance_m, 5.0, 60.0)?;
        f("shot.xg.a", sh.xg.a, -20.0, 30.0)?;
        f("shot.xg.b_ln_dist", sh.xg.b_ln_dist, 0.0, 10.0)?;
        f("shot.xg.c_angle", sh.xg.c_angle, 0.0, 10.0)?;
        f("shot.xg.pressure", sh.xg.pressure, 0.0, 10.0)?;
        f("shot.xg.skill_gain", sh.xg.skill_gain, 0.0, 10.0)?;
        f("shot.xg.min_dist_m", sh.xg.min_dist_m, 1.0, 20.0)?;
        f("shot.goal_value", sh.goal_value, 0.1, 10.0)?;
        f("shot.miss_cost", sh.miss_cost, 0.0, 0.1)?;
        f("shot.aim_inset_m", sh.aim_inset_m, 0.0, 3.0)?;
        f("shot.aim_height_m", sh.aim_height_m, 0.1, 2.4)?;
        f("shot.far_corner_share", sh.far_corner_share, 0.0, 1.0)?;
        f("shot.speed_min_ms", sh.speed_min_ms, 10.0, 40.0)?;
        f("shot.speed_max_ms", sh.speed_max_ms, 10.0, 45.0)?;
        o(
            "speed_min_ms",
            "speed_max_ms",
            sh.speed_min_ms,
            sh.speed_max_ms,
        )?;
        f(
            "shot.angle_sigma_best_deg",
            sh.angle_sigma_best_deg,
            0.0,
            20.0,
        )?;
        f(
            "shot.angle_sigma_worst_deg",
            sh.angle_sigma_worst_deg,
            0.0,
            30.0,
        )?;
        o(
            "angle_sigma_best_deg",
            "angle_sigma_worst_deg",
            sh.angle_sigma_best_deg,
            sh.angle_sigma_worst_deg,
        )?;
        f("shot.pressure_gain", sh.pressure_gain, 0.0, 5.0)?;
        f("shot.long_shot_gain", sh.long_shot_gain, -0.1, 0.1)?;
        let sv = &sh.save;
        f("shot.save.reach_m", sv.reach_m, 0.5, 4.0)?;
        f("shot.save.reach_height_m", sv.reach_height_m, 0.5, 4.0)?;
        f("shot.save.base", sv.base, -10.0, 10.0)?;
        f("shot.save.gain", sv.gain, 0.0, 20.0)?;
        f("shot.save.speed_cost", sv.speed_cost, 0.0, 2.0)?;
        f("shot.save.catch_share", sv.catch_share, 0.0, 1.0)?;
        f("shot.save.parry_speed_ms", sv.parry_speed_ms, 0.0, 20.0)?;
        f("shot.save.lockout_s", sv.lockout_s, 0.0, 5.0)?;
        f("restart.dead_s", c.restart.dead_s, 0.0, 30.0)?;
        f("restart.goal_dead_s", c.restart.goal_dead_s, 0.0, 30.0)?;
        f("restart.arrive_m", c.restart.arrive_m, 0.1, 10.0)?;
        Ok(c)
    }
}
