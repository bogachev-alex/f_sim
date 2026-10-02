use super::{check_order, check_range, parse, ConfigError};
use serde::Deserialize;

const FILE: &str = "defense.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Press {
    /// В матче оборона встречает владельца у своих ворот независимо от стиля: мяч глубже
    /// `engage_zone_m` от своих ворот и защитник ближе `engage_radius_m` к нему.
    pub engage_zone_m: f32,
    pub engage_radius_m: f32,
    /// Дальше этой глубины мяча (от своих ворот обороняющейся команды) команда не прессингует:
    /// граница растёт с `block_height` от `zone_min_m` до `zone_max_m`.
    pub zone_min_m: f32,
    pub zone_max_m: f32,
    /// Выше этой `press_intensity` команда прессингует всегда в своей зоне, ниже только по триггерам.
    pub always_threshold: f32,
    /// Сколько длится прессинг после срабатывания триггера, с: от `duration_min_s` до `duration_max_s`.
    pub duration_min_s: f32,
    pub duration_max_s: f32,
    /// Максимум игроков поддержки (тени прикрытия) при `press_intensity` 1.
    pub max_support: u32,
    /// Страхующий появляется при `press_intensity` не ниже этого значения.
    pub cover_threshold: f32,
    /// Упреждение по скорости владельца, с, и скорость движения к своим воротам, с которой
    /// прессингующий не бежит сзади, а встаёт на пути владельца к воротам.
    pub lead_s: f32,
    pub contain_speed_ms: f32,
    /// Прессингующий встаёт на линию паса на таком расстоянии от владельца.
    pub cut_offset_m: f32,
    /// Минимальное расстояние до владельца: ближе не подходят.
    pub contact_m: f32,
    /// Тень прикрытия: доля длины линии паса от владельца и минимальное расстояние до него.
    pub support_lane_frac: f32,
    pub support_min_m: f32,
    /// Страхующий стоит за прессингующим на таком расстоянии в сторону своих ворот.
    pub cover_back_m: f32,
    /// Триггер «мяч у бровки»: ближе этого расстояния до боковой линии.
    pub touchline_m: f32,
    /// Триггер «пас назад»: мяч движется к воротам владеющей команды быстрее этой скорости.
    pub back_pass_speed_ms: f32,
    /// Триггер «пас вратарю»: владеет вратарь, мяч ближе этого расстояния к воротам владеющей команды.
    pub keeper_zone_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterPress {
    /// Ниже этого `counter_press` контрпрессинга нет.
    pub min: f32,
    pub duration_min_s: f32,
    pub duration_max_s: f32,
    /// Игроков в контрпрессинге при `counter_press` 1 (включая прессингующего).
    pub players_max: u32,
    pub ring_radius_m: f32,
    pub ring_spread_deg: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Marking {
    pub max_marks: u32,
    /// Глубина опасной зоны от своих ворот при зональной (`marking` 0) и персональной (1) опеке.
    pub zone_depth_zonal_m: f32,
    pub zone_depth_man_m: f32,
    /// Опекун стоит между подопечным и своими воротами на таком расстоянии от него.
    pub goalside_m: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assign {
    /// Цена смены места в формации и дополнительная цена при смене линии (защитник на место нападающего).
    pub swap_base_s: f32,
    pub group_mismatch_s: f32,
    /// Цена выхода из линии обороны на прессинг для флангового и для центрального защитника.
    pub back_leave_cost_s: f32,
    pub cb_leave_cost_s: f32,
    /// Центральные защитники выходят на прессинг только при `press_intensity` не ниже порога.
    pub cb_press_threshold: f32,
    /// Бонус за сохранение прошлого назначения; убывает с коммуникацией линии.
    pub stick_min_s: f32,
    pub stick_max_s: f32,
    /// Бонус обязанностям (прессинг и т. п.), чтобы они заполнялись раньше зональных мест.
    pub special_bonus_s: f32,
    /// Задержка реакции в оценке времени прибытия.
    pub react_s: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    /// Владельцем остаётся прежний игрок, пока другой не ближе к мячу на этот запас.
    pub switch_margin_m: f32,
}

/// Оборона без мяча (`docs/04-off-ball.md` §2, §5).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseConfig {
    /// Период пересчёта назначений, с.
    pub period_s: f32,
    pub press: Press,
    pub counter_press: CounterPress,
    pub marking: Marking,
    pub assign: Assign,
    pub owner: Owner,
}

impl DefenseConfig {
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let c: DefenseConfig = parse(FILE, json)?;
        let f = |n: &str, v: f32, lo: f64, hi: f64| check_range(FILE, n, v as f64, lo, hi);
        let o = |a: &str, b: &str, x: f32, y: f32| check_order(FILE, a, b, x as f64, y as f64);
        f("period_s", c.period_s, 0.05, 5.0)?;
        let p = &c.press;
        f("press.engage_zone_m", p.engage_zone_m, 0.0, 105.0)?;
        f("press.engage_radius_m", p.engage_radius_m, 0.0, 60.0)?;
        f("press.zone_min_m", p.zone_min_m, 0.0, 105.0)?;
        f("press.zone_max_m", p.zone_max_m, 0.0, 120.0)?;
        o("zone_min_m", "zone_max_m", p.zone_min_m, p.zone_max_m)?;
        f("press.always_threshold", p.always_threshold, 0.0, 1.0)?;
        f("press.duration_min_s", p.duration_min_s, 0.1, 60.0)?;
        f("press.duration_max_s", p.duration_max_s, 0.1, 60.0)?;
        o(
            "duration_min_s",
            "duration_max_s",
            p.duration_min_s,
            p.duration_max_s,
        )?;
        check_range(FILE, "press.max_support", p.max_support as f64, 0.0, 4.0)?;
        f("press.cover_threshold", p.cover_threshold, 0.0, 1.0)?;
        f("press.lead_s", p.lead_s, 0.0, 2.0)?;
        f("press.contain_speed_ms", p.contain_speed_ms, 0.0, 15.0)?;
        f("press.cut_offset_m", p.cut_offset_m, 0.0, 10.0)?;
        f("press.contact_m", p.contact_m, 0.0, 5.0)?;
        f("press.support_lane_frac", p.support_lane_frac, 0.0, 1.0)?;
        f("press.support_min_m", p.support_min_m, 0.0, 20.0)?;
        f("press.cover_back_m", p.cover_back_m, 0.0, 30.0)?;
        f("press.touchline_m", p.touchline_m, 0.0, 30.0)?;
        f("press.back_pass_speed_ms", p.back_pass_speed_ms, 0.0, 30.0)?;
        f("press.keeper_zone_m", p.keeper_zone_m, 0.0, 60.0)?;
        let cp = &c.counter_press;
        f("counter_press.min", cp.min, 0.0, 1.0)?;
        f("counter_press.duration_min_s", cp.duration_min_s, 0.1, 30.0)?;
        f("counter_press.duration_max_s", cp.duration_max_s, 0.1, 30.0)?;
        o(
            "duration_min_s",
            "duration_max_s",
            cp.duration_min_s,
            cp.duration_max_s,
        )?;
        check_range(
            FILE,
            "counter_press.players_max",
            cp.players_max as f64,
            1.0,
            6.0,
        )?;
        f("counter_press.ring_radius_m", cp.ring_radius_m, 1.0, 20.0)?;
        f(
            "counter_press.ring_spread_deg",
            cp.ring_spread_deg,
            0.0,
            180.0,
        )?;
        let m = &c.marking;
        check_range(FILE, "marking.max_marks", m.max_marks as f64, 0.0, 8.0)?;
        f(
            "marking.zone_depth_zonal_m",
            m.zone_depth_zonal_m,
            5.0,
            105.0,
        )?;
        f("marking.zone_depth_man_m", m.zone_depth_man_m, 5.0, 105.0)?;
        o(
            "zone_depth_zonal_m",
            "zone_depth_man_m",
            m.zone_depth_zonal_m,
            m.zone_depth_man_m,
        )?;
        f("marking.goalside_m", m.goalside_m, 0.0, 10.0)?;
        let a = &c.assign;
        f("assign.swap_base_s", a.swap_base_s, 0.0, 10.0)?;
        f("assign.group_mismatch_s", a.group_mismatch_s, 0.0, 20.0)?;
        f("assign.back_leave_cost_s", a.back_leave_cost_s, 0.0, 20.0)?;
        f("assign.cb_leave_cost_s", a.cb_leave_cost_s, 0.0, 30.0)?;
        o(
            "back_leave_cost_s",
            "cb_leave_cost_s",
            a.back_leave_cost_s,
            a.cb_leave_cost_s,
        )?;
        f("assign.cb_press_threshold", a.cb_press_threshold, 0.0, 1.0)?;
        f("assign.stick_min_s", a.stick_min_s, 0.0, 10.0)?;
        f("assign.stick_max_s", a.stick_max_s, 0.0, 10.0)?;
        o("stick_min_s", "stick_max_s", a.stick_min_s, a.stick_max_s)?;
        f("assign.special_bonus_s", a.special_bonus_s, 100.0, 1.0e6)?;
        f("assign.react_s", a.react_s, 0.0, 2.0)?;
        f("owner.switch_margin_m", c.owner.switch_margin_m, 0.0, 20.0)?;
        Ok(c)
    }
}
