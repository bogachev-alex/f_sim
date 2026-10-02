use fsim_core::config::{Config, ConfigError, PerfConfig, PhysicsConfig};
use std::path::Path;

fn config_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config")
}

#[test]
fn shipped_config_loads() {
    let c = Config::load_dir(&config_dir()).expect("config/ должен загружаться");
    assert_eq!(c.physics.time.tick_hz, 20);
    assert_eq!(c.composites.0.len(), 13);
    assert_eq!(c.attributes.all().count(), 9 + 14 + 12 + 8 + 5);
}

fn physics_json() -> String {
    std::fs::read_to_string(config_dir().join("physics.json")).unwrap()
}

#[test]
fn out_of_range_value_fails() {
    let bad = physics_json().replace("\"gravity_ms2\": 9.81", "\"gravity_ms2\": 50.0");
    assert!(matches!(
        PhysicsConfig::from_json(&bad),
        Err(ConfigError::Range { .. })
    ));
}

#[test]
fn min_greater_than_max_fails() {
    let bad = physics_json().replace("\"max_speed_min_ms\": 7.0", "\"max_speed_min_ms\": 11.0");
    assert!(matches!(
        PhysicsConfig::from_json(&bad),
        Err(ConfigError::Invalid { .. })
    ));
}

#[test]
fn unknown_field_fails() {
    let bad = physics_json().replace("\"pitch\": {", "\"pitch\": { \"typo\": 1,");
    assert!(matches!(
        PhysicsConfig::from_json(&bad),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn missing_field_fails() {
    assert!(matches!(
        PerfConfig::from_json("{}"),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn composite_weights_must_sum_to_one() {
    let dir = config_dir();
    let attrs = std::fs::read_to_string(dir.join("attributes.json")).unwrap();
    let attrs = fsim_core::config::Attributes::from_json(&attrs).unwrap();
    let bad = r#"{"x": {"passing": 0.5, "vision": 0.2}}"#;
    assert!(fsim_core::config::Composites::from_json(bad, &attrs).is_err());
    let unknown = r#"{"x": {"nope": 1.0}}"#;
    assert!(fsim_core::config::Composites::from_json(unknown, &attrs).is_err());
}

#[test]
fn attr_enum_matches_attributes_json() {
    use fsim_core::attrs::Attr;
    let c = Config::load_dir(&config_dir()).unwrap();
    let from_json: Vec<&str> = c.attributes.all().collect();
    assert_eq!(
        from_json,
        Attr::NAMES,
        "порядок и имена в attributes.json и в enum Attr"
    );
    // Группа атрибута в enum совпадает с группой в JSON.
    for (group, names) in [
        ("physical", &c.attributes.physical),
        ("technical", &c.attributes.technical),
        ("mental", &c.attributes.mental),
        ("goalkeeping", &c.attributes.goalkeeping),
        ("traits", &c.attributes.traits),
    ] {
        for n in names {
            assert_eq!(Attr::from_name(n).unwrap().group(), group, "{n}");
        }
    }
}

#[test]
fn response_curves_are_monotone_with_fixed_ends() {
    use fsim_core::attrs::Attr;
    use fsim_core::curves::CurveSet;
    let c = Config::load_dir(&config_dir()).unwrap();
    let set = CurveSet::build(&c.curves).unwrap();
    for &a in Attr::ALL {
        assert!(set.eval(a, 0.0).abs() < 1e-5, "{}: f(0)", a.name());
        assert!((set.eval(a, 1.0) - 1.0).abs() < 1e-5, "{}: f(1)", a.name());
        let mut prev = 0.0f32;
        for i in 0..=1000 {
            let y = set.eval(a, i as f32 / 1000.0);
            assert!(y >= prev - 1e-6, "{} не монотонна в {i}", a.name());
            prev = y;
        }
    }
    // Физическая кривая усиливает верх: разрыв между 0,8 и 0,9 больше, чем у линейной.
    let phys = set.eval(Attr::pace, 0.9) - set.eval(Attr::pace, 0.8);
    assert!(phys > 0.1);
    // Черты линейные.
    assert!((set.eval(Attr::consistency, 0.37) - 0.37).abs() < 1e-3);
}

#[test]
fn bad_curve_config_fails() {
    use fsim_core::config::CurvesConfig;
    let bad_order = r#"{"groups":{"p":{"type":"piecewise","knots":[[0,0],[0.5,0.8],[0.7,0.4],[1,1]]}},"overrides":{}}"#;
    assert!(CurvesConfig::from_json(bad_order).is_err());
    let bad_ends =
        r#"{"groups":{"p":{"type":"piecewise","knots":[[0.1,0],[1,1]]}},"overrides":{}}"#;
    assert!(CurvesConfig::from_json(bad_ends).is_err());
    let bad_steep =
        r#"{"groups":{"p":{"type":"logistic","center":0.5,"steepness":99}},"overrides":{}}"#;
    assert!(CurvesConfig::from_json(bad_steep).is_err());
}

#[test]
fn embedded_config_equals_directory_config() {
    let a = Config::load_dir(&config_dir()).unwrap();
    let b = fsim_core::embedded::config().unwrap();
    assert_eq!(a.physics.pitch.length_m, b.physics.pitch.length_m);
    assert_eq!(a.sandbox.uniform_attr, b.sandbox.uniform_attr);
}

#[test]
fn embedded_files_match_config_directory() {
    // Вшитый список должен совпадать с каталогами: новый стиль, роль или сценарий без записи
    // в `embedded.rs` не попадёт в WASM.
    let dir = config_dir();
    let a = Config::load_dir(&dir).unwrap();
    let b = fsim_core::embedded::config().unwrap();
    assert_eq!(
        a.styles.keys().collect::<Vec<_>>(),
        b.styles.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        a.roles.keys().collect::<Vec<_>>(),
        b.roles.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        a.scenarios.keys().collect::<Vec<_>>(),
        b.scenarios.keys().collect::<Vec<_>>()
    );
}

#[test]
fn styles_roles_and_scenarios_are_complete() {
    let c = Config::load_dir(&config_dir()).unwrap();
    assert!(c.roles.len() >= 20, "ролей {}", c.roles.len());
    for name in [
        "positional",
        "gegenpress",
        "low_block_counter",
        "direct_long_ball",
        "wing_play",
        "balanced",
    ] {
        assert!(c.styles.contains_key(name), "нет пресета {name}");
    }
    // Значения пресетов из таблицы `docs/05-styles.md` §2.
    let p = &c.styles["positional"];
    assert_eq!(
        (p.buildup_short, p.line_height, p.counter_attack),
        (0.95, 0.80, 0.25)
    );
    let g = &c.styles["gegenpress"];
    assert_eq!(
        (g.press_intensity, g.counter_press, g.patience),
        (0.95, 0.95, 0.30)
    );
    let l = &c.styles["low_block_counter"];
    assert_eq!(
        (l.line_height, l.block_height, l.counter_attack),
        (0.15, 0.15, 0.95)
    );
    assert!(c.scenarios.len() >= 4);
}

#[test]
fn bad_style_and_scenario_are_rejected() {
    use fsim_core::config::{Scenario, Style};
    let good = std::fs::read_to_string(config_dir().join("styles/balanced.json")).unwrap();
    assert!(Style::from_json("s", &good).is_ok());
    let bad_range = good.replace("\"tempo\": 0.55", "\"tempo\": 1.5");
    assert!(Style::from_json("s", &bad_range).is_err());
    let bad_sum = good.replace("\"crosses\": 0.25", "\"crosses\": 0.9");
    assert!(Style::from_json("s", &bad_sum).is_err());
    let unknown = good.replace("\"tempo\"", "\"tempo_typo\"");
    assert!(Style::from_json("s", &unknown).is_err());

    let sc = std::fs::read_to_string(config_dir().join("scenarios/ball_sweep.json")).unwrap();
    assert!(Scenario::from_json("sc", &sc, 52.5, 34.0).is_ok());
    let outside = sc.replace("\"x\": 45", "\"x\": 99");
    assert!(Scenario::from_json("sc", &outside, 52.5, 34.0).is_err());
    let late = sc.replace("\"duration_s\": 60.0", "\"duration_s\": 90.0");
    assert!(
        Scenario::from_json("sc", &late, 52.5, 34.0).is_err(),
        "траектория короче сценария"
    );
}
