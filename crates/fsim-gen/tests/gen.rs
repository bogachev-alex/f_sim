use fsim_core::attrs::Attr;
use fsim_core::config::Formations;
use fsim_core::data::{Position, Squad};
use fsim_gen::{generate_equal_pair, generate_squad, GenConfig, SquadParams};

fn load() -> (GenConfig, Formations) {
    let g = GenConfig::from_json(include_str!("../../../config/generator.json")).unwrap();
    let f = Formations::from_json(include_str!("../../../config/formations.json")).unwrap();
    (g, f)
}

fn mean(xs: impl Iterator<Item = f32>) -> f32 {
    let (s, n) = xs.fold((0.0f64, 0usize), |(s, n), x| (s + x as f64, n + 1));
    (s / n as f64) as f32
}

fn corr(xs: &[f32], ys: &[f32]) -> f32 {
    let (mx, my) = (mean(xs.iter().copied()), mean(ys.iter().copied()));
    let (mut sxy, mut sxx, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in xs.iter().zip(ys) {
        let (dx, dy) = ((x - mx) as f64, (y - my) as f64);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    (sxy / (sxx * syy).sqrt()) as f32
}

fn all_players(n: u64, quality: Option<f32>) -> Vec<fsim_core::data::PlayerData> {
    let (g, f) = load();
    let mut out = Vec::new();
    for seed in 0..n {
        let mut p = SquadParams::new(seed, "T", "4-3-3");
        p.quality = quality;
        out.extend(generate_squad(&g, &f, &p).unwrap().players);
    }
    out
}

#[test]
fn squad_shape_and_ranges() {
    let (g, f) = load();
    for formation in f.0.keys() {
        let s = generate_squad(&g, &f, &SquadParams::new(1, "T", formation)).unwrap();
        assert_eq!(s.players.len(), 11 + 7, "{formation}");
        for (pl, slot) in s.players.iter().zip(&f.0[formation]) {
            assert_eq!(pl.position, slot.pos);
        }
        for pl in &s.players {
            assert_eq!(pl.attrs.len(), 48);
            assert!(pl
                .attrs
                .iter()
                .all(|v| (g.attr_floor..=g.attr_ceil).contains(v)));
            assert_eq!(pl.position_ratings[pl.position as usize], 1.0);
            assert!(pl.position_ratings.iter().all(|v| (0.0..=1.0).contains(v)));
            assert!((150.0..=210.0).contains(&pl.height_cm));
            assert!(
                (55.0..=100.0).contains(&pl.weight_kg),
                "вес {}",
                pl.weight_kg
            );
        }
    }
}

#[test]
fn unknown_formation_is_an_error() {
    let (g, f) = load();
    assert!(generate_squad(&g, &f, &SquadParams::new(1, "T", "9-9-9")).is_err());
}

#[test]
fn same_seed_same_squad_json() {
    let (g, f) = load();
    let a =
        serde_json::to_string(&generate_squad(&g, &f, &SquadParams::new(5, "T", "4-4-2")).unwrap())
            .unwrap();
    let b =
        serde_json::to_string(&generate_squad(&g, &f, &SquadParams::new(5, "T", "4-4-2")).unwrap())
            .unwrap();
    let c =
        serde_json::to_string(&generate_squad(&g, &f, &SquadParams::new(6, "T", "4-4-2")).unwrap())
            .unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
    // Круг через JSON не теряет данные.
    let back: Squad = serde_json::from_str(&a).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), a);
}

#[test]
fn quality_shifts_attributes() {
    let low = all_players(30, Some(0.45));
    let high = all_players(30, Some(0.75));
    let avg = |ps: &[fsim_core::data::PlayerData]| {
        mean(
            ps.iter()
                .filter(|p| p.position != Position::GK)
                .map(|p| p.attr(Attr::passing)),
        )
    };
    assert!(avg(&high) > avg(&low) + 0.2);
}

#[test]
fn height_correlations() {
    let ps = all_players(80, None);
    let h: Vec<f32> = ps.iter().map(|p| p.height_cm).collect();
    let col = |a: Attr| ps.iter().map(|p| p.attr(a)).collect::<Vec<f32>>();
    assert!(
        corr(&h, &col(Attr::jumping_reach)) > 0.15,
        "рост и jumping_reach"
    );
    assert!(corr(&h, &col(Attr::strength)) > 0.1, "рост и strength");
    assert!(corr(&h, &col(Attr::agility)) < -0.1, "рост и agility");
}

#[test]
fn position_profiles_are_distinct() {
    let ps = all_players(40, None);
    let by =
        |pos: Position, a: Attr| mean(ps.iter().filter(|p| p.position == pos).map(|p| p.attr(a)));
    assert!(by(Position::GK, Attr::reflexes) > by(Position::ST, Attr::reflexes) + 0.4);
    assert!(by(Position::ST, Attr::finishing) > by(Position::CB, Attr::finishing) + 0.2);
    assert!(by(Position::CB, Attr::tackling) > by(Position::W, Attr::tackling) + 0.15);
    assert!(by(Position::W, Attr::pace) > by(Position::CB, Attr::pace));
}

/// «Одинаковые составы»: средние по атрибутам у команд A и B статистически неразличимы.
#[test]
fn equal_pair_is_statistically_equivalent() {
    let (g, f) = load();
    let n = 200;
    let mut sums = [[0.0f64; 48]; 2];
    for seed in 0..n {
        let (a, b) = generate_equal_pair(&g, &f, seed, "4-3-3", None).unwrap();
        assert_ne!(
            serde_json::to_string(&a.players[0]).unwrap(),
            serde_json::to_string(&b.players[0]).unwrap()
        );
        for (k, s) in [a, b].iter().enumerate() {
            for p in s.players.iter().take(11) {
                for (i, v) in p.attrs.iter().enumerate() {
                    sums[k][i] += *v as f64;
                }
            }
        }
    }
    let cnt = (n * 11) as f64;
    for (i, name) in Attr::NAMES.iter().enumerate() {
        let d = (sums[0][i] - sums[1][i]).abs() / cnt;
        assert!(d < 0.015, "{name}: разница средних {d}");
    }
}

#[test]
fn bias_shifts_attribute() {
    let (g, f) = load();
    let mean_first_touch = |bias: Vec<(Attr, f32)>| {
        let mut ps = Vec::new();
        for seed in 0..40 {
            let mut p = SquadParams::new(seed, "T", "4-3-3");
            p.bias = bias.clone();
            ps.extend(generate_squad(&g, &f, &p).unwrap().players);
        }
        mean(
            ps.iter()
                .filter(|p| p.position != Position::GK)
                .map(|p| p.attr(Attr::first_touch)),
        )
    };
    let d = mean_first_touch(vec![(Attr::first_touch, 0.15)]) - mean_first_touch(vec![]);
    assert!((d - 0.15).abs() < 0.03, "смещение {d}");
}
