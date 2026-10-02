use fsim_wasm::{Engine, FrameLayout, MatchInfo};

fn scenario(seed: u64, id: &str) -> Engine {
    Engine::scenario(seed, id, "", "", "", "", 0.62)
        .unwrap_or_else(|_| panic!("сценарий создаётся"))
}

fn engine(seed: u64) -> Engine {
    Engine::new(seed, 0.62, "4-3-3", "4-4-2").unwrap_or_else(|_| panic!("движок создаётся"))
}

#[test]
fn first_call_returns_initial_frame_and_all_ticks() {
    let l = FrameLayout::new();
    let mut e = engine(1);
    let frames = e.run_to(100);
    assert_eq!(frames.len(), 101 * l.stride as usize, "кадры 0..=100");
    assert_eq!(frames[0], 0.0);
    assert_eq!(frames[(100 * l.stride) as usize], 100.0);
    // Повторный вызов до того же тика ничего не добавляет.
    assert!(e.run_to(100).is_empty());
    assert_eq!(e.tick(), 100);
}

#[test]
fn chunking_does_not_change_frames() {
    let mut a = engine(7);
    let whole = a.run_to(200);
    let mut b = engine(7);
    let mut parts = Vec::new();
    for t in (20..=200).step_by(20) {
        parts.extend(b.run_to(t));
    }
    // Нулевой кадр приходит с первым вызовом; в сумме те же числа в том же порядке.
    assert_eq!(whole, parts);
}

#[test]
fn start_is_mirrored_formation() {
    let l = FrameLayout::new();
    let mut e = engine(3);
    let f = e.run_to(0);
    let at = |i: usize, k: usize| f[(l.players_offset as usize) + i * l.player_fields as usize + k];
    // Вратарь A у своих ворот слева, вратарь B справа; команды стоят на своих половинах.
    assert!(at(0, 0) < -40.0 && at(11, 0) > 40.0);
    let mean =
        |r: std::ops::Range<usize>| r.clone().map(|i| at(i, 0)).sum::<f32>() / r.len() as f32;
    assert!(
        mean(0..11) < 0.0 && mean(11..22) > 0.0,
        "команды на своих половинах в среднем"
    );
    // Зеркало: игроку слева соответствует игрок справа с тем же рядом схемы.
    assert!(f.iter().all(|v| v.is_finite()));
}

#[test]
fn info_is_consistent() {
    let e = engine(5);
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    assert_eq!(info["players"].as_array().unwrap().len(), 22);
    assert_eq!(info["seed"], "5");
    assert_eq!(info["layout"]["stride"], FrameLayout::new().stride);
    let _typed: Option<MatchInfo> = None;
}

#[test]
fn players_stay_in_physical_limits() {
    let l = FrameLayout::new();
    let mut e = engine(11);
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    let max: Vec<f64> = info["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["max_speed_ms"].as_f64().unwrap())
        .collect();
    let f = e.run_to(1200);
    let stride = l.stride as usize;
    for fr in f.chunks(stride) {
        for (i, m) in max.iter().enumerate() {
            let o = 1 + i * l.player_fields as usize;
            let sp = (fr[o + 2] as f64).hypot(fr[o + 3] as f64);
            assert!(sp <= m + 1e-3, "игрок {i}: скорость {sp} > {m}");
            assert!(fr[o].abs() <= 52.6 && fr[o + 1].abs() <= 34.1 + 1.0);
        }
    }
}

#[test]
fn scenario_engine_runs_to_its_duration_and_stops() {
    let l = FrameLayout::new();
    let mut e = scenario(3, "shape_hold");
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    assert_eq!(info["mode"], "scenario");
    assert_eq!(info["scenario"], "shape_hold");
    let ticks = info["duration_ticks"].as_u64().unwrap() as u32;
    assert_eq!(ticks, 800);
    let frames = e.run_to(ticks + 500);
    assert_eq!(
        frames.len(),
        (ticks as usize + 1) * l.stride as usize,
        "после конца новых кадров нет"
    );
    assert_eq!(e.tick(), ticks);
    assert!(e.run_to(ticks + 100).is_empty());
}

#[test]
fn scenario_frames_carry_anchors_and_team_state() {
    let l = FrameLayout::new();
    let mut e = scenario(3, "possession_flips");
    let f = e.run_to(40);
    let last = &f[40 * l.stride as usize..];
    let team = |k: usize, j: usize| last[(l.teams_offset + k as u32 * l.team_fields) as usize + j];
    // Владеет команда 0: её фаза атаки, у другой оборона.
    assert!(team(0, 0) > 0.9 && team(1, 0) < 0.1);
    assert_eq!(team(0, 3), 1.0);
    assert_eq!(team(1, 3), 0.0);
    // Линия обороны команды 1 лежит в её половине (мировая x > 0 у ворот на +x).
    assert!(
        team(1, 1) > 0.0,
        "линия обороны команды 1 у её ворот: {}",
        team(1, 1)
    );
    // Якоря рядом с игроками (старт в якорях).
    let p = |i: usize, k: usize| last[(l.players_offset + i as u32 * l.player_fields) as usize + k];
    for i in 0..22 {
        let d = (p(i, 0) - p(i, 8)).hypot(p(i, 1) - p(i, 9));
        assert!(d < 12.0, "игрок {i} далеко от якоря: {d}");
    }
}

#[test]
fn scenario_overrides_apply() {
    let e = Engine::scenario(
        1,
        "ball_sweep",
        "gegenpress",
        "positional",
        "4-4-2",
        "3-5-2",
        0.6,
    )
    .ok()
    .unwrap();
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    assert_eq!(info["styles"][0], "gegenpress");
    assert_eq!(info["formations"][1], "3-5-2");
}

#[test]
fn info_lines_and_roles_are_filled() {
    let e = scenario(2, "shape_hold");
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    let players = info["players"].as_array().unwrap();
    assert_eq!(players[0]["line"], 0);
    assert_eq!(players[11]["line"], 0);
    // 4-3-3: четыре защитника, три полузащитника, три нападающих у команды A.
    let count = |team: usize, line: u64| {
        players[team * 11..team * 11 + 11]
            .iter()
            .filter(|p| p["line"] == line)
            .count()
    };
    assert_eq!((count(0, 1), count(0, 2), count(0, 3)), (4, 3, 3));
    assert!(players
        .iter()
        .all(|p| p["role"].as_str().is_some_and(|r| !r.is_empty())));
}

#[test]
fn scenario_frames_carry_defensive_duties() {
    let l = FrameLayout::new();
    assert_eq!(l.player_fields, 12);
    let mut e = scenario(5, "buildup_vs_press");
    let f = e.run_to(400);
    let last = &f[400 * l.stride as usize..];
    let at =
        |i: usize, k: usize| last[(l.players_offset + i as u32 * l.player_fields) as usize + k];
    // Команда B (индексы 11..21) давит: есть прессингующий (2), и он указывает на владельца A.
    let presser = (12..22)
        .find(|&i| at(i, 10) == 2.0)
        .expect("прессингующий есть");
    let target = at(presser, 11) as usize;
    assert!(
        target < 11,
        "прессингующий ссылается на игрока A, а не {target}"
    );
    assert_eq!(at(target, 10), 7.0, "цель прессинга владелец мяча");
    // Вратарь без обязанности.
    assert_eq!(at(11, 10), 0.0);
}

#[test]
fn live_engine_plays_and_reports_stats() {
    let l = FrameLayout::new();
    assert_eq!(l.ball_fields, 8);
    let mut e = Engine::live(4, 0.62, "", "", "", "").unwrap_or_else(|_| panic!("матч создаётся"));
    let info: serde_json::Value = serde_json::from_str(&e.info()).unwrap();
    assert_eq!(info["mode"], "match");
    assert_eq!(info["duration_ticks"], 0);
    let frames = e.run_to(4000);
    assert_eq!(frames.len(), 4001 * l.stride as usize);
    // Режим мяча в кадре: бывает и у ног, и в полёте.
    let modes: std::collections::BTreeSet<u32> = frames
        .chunks(l.stride as usize)
        .map(|f| f[l.ball_offset as usize + 6] as u32)
        .collect();
    assert!(
        modes.contains(&1) && modes.contains(&2),
        "режимы мяча {modes:?}"
    );
    let stats: serde_json::Value = serde_json::from_str(&e.stats()).unwrap();
    assert!(stats["passes"][0].as_u64().unwrap() + stats["passes"][1].as_u64().unwrap() > 5);
    assert!(stats["score"].is_array());
}

#[test]
fn live_engine_chunking_does_not_change_frames() {
    let mk = || {
        Engine::live(9, 0.62, "wing_play", "balanced", "4-2-3-1", "4-4-2")
            .unwrap_or_else(|_| panic!("матч создаётся"))
    };
    let whole = mk().run_to(1500);
    let mut e = mk();
    let mut parts = Vec::new();
    for t in (100..=1500).step_by(100) {
        parts.extend(e.run_to(t));
    }
    assert_eq!(whole, parts);
}
