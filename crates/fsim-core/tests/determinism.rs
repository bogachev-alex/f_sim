//! Детерминизм: один seed и один конфиг дают побитово один матч.
//! Золотое значение сверяется и с WASM-сборкой (`just wasm-determinism`).

use fsim_core::embedded;
use fsim_core::live_match::run_match;
use fsim_core::match_loop::run_sandbox;
use fsim_core::rng::match_seed;
use fsim_core::scenario_sim::run_scenario;

/// Хэш песочницы для seed 42 и 2000 тиков. Менять только осознанно, вместе с WASM-проверкой.
pub const GOLDEN: u64 = 0x8653_9668_c05f_98fa;

#[test]
fn repeatable() {
    let c = embedded::config().unwrap();
    assert_eq!(run_sandbox(42, 2000, &c), run_sandbox(42, 2000, &c));
}

#[test]
fn different_seeds_differ() {
    let c = embedded::config().unwrap();
    assert_ne!(run_sandbox(1, 500, &c), run_sandbox(2, 500, &c));
}

#[test]
fn batch_order_does_not_matter() {
    let c = embedded::config().unwrap();
    let fwd: Vec<u64> = (0..8)
        .map(|i| run_sandbox(match_seed(7, i), 300, &c))
        .collect();
    let mut rev: Vec<u64> = (0..8)
        .rev()
        .map(|i| run_sandbox(match_seed(7, i), 300, &c))
        .collect();
    rev.reverse();
    assert_eq!(fwd, rev);
}

#[test]
fn golden_value() {
    let h = run_sandbox(42, 2000, &embedded::config().unwrap());
    println!("hash = {h:#018x}");
    assert_eq!(
        h, GOLDEN,
        "хэш изменился: проверь, что это намеренно, и обнови GOLDEN"
    );
}

/// Хэш сценария `possession_flips` для seed 42 и 2000 тиков; сверяется с WASM.
pub const GOLDEN_SCENARIO: u64 = 0x9b8b_2ac2_7a45_f18d;

#[test]
fn scenario_golden_value() {
    let c = embedded::config().unwrap();
    let h = run_scenario(42, 2000, &c, "possession_flips").unwrap();
    println!("scenario hash = {h:#018x}");
    assert_eq!(
        h, GOLDEN_SCENARIO,
        "хэш сценария изменился: проверь и обнови GOLDEN_SCENARIO"
    );
}

#[test]
fn scenario_repeatable_and_seed_dependent() {
    let c = embedded::config().unwrap();
    let a = run_scenario(1, 600, &c, "switch_play").unwrap();
    assert_eq!(a, run_scenario(1, 600, &c, "switch_play").unwrap());
    // Seed влияет через рассеяние линии обороны.
    assert_ne!(a, run_scenario(2, 600, &c, "switch_play").unwrap());
}

/// Хэш матча `positional` против `gegenpress` (4-3-3), seed 42, 6000 тиков; сверяется с WASM.
pub const GOLDEN_MATCH: u64 = 0x4089_9a1e_824b_8a8a;

fn match_hash() -> u64 {
    let c = embedded::config().unwrap();
    run_match(
        42,
        6000,
        &c,
        ["positional", "gegenpress"],
        ["4-3-3", "4-3-3"],
    )
    .unwrap()
}

#[test]
fn match_golden_value() {
    let h = match_hash();
    println!("match hash = {h:#018x}");
    assert_eq!(
        h, GOLDEN_MATCH,
        "хэш матча изменился: проверь и обнови GOLDEN_MATCH"
    );
}
