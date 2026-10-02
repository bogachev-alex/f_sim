//! Тик не должен выделять память после прогрева (`perf.json: tick_allocs_max`).

use fsim_core::config::Config;
use fsim_core::match_loop::MatchSim;
use fsim_core::sink::NoopSink;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::Path;

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCS.with(|c| c.set(c.get() + 1));
        System.realloc(p, l, n)
    }
}

#[global_allocator]
static A: Counting = Counting;

fn allocs() -> u64 {
    ALLOCS.with(|c| c.get())
}

#[test]
fn tick_loop_does_not_allocate_after_warmup() {
    let cfg =
        Config::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config")).unwrap();
    let mut sim = MatchSim::sandbox(42, &cfg, NoopSink);
    sim.run(100); // прогрев
    let before = allocs();
    sim.run(5_000);
    let used = allocs() - before;
    assert!(
        used <= cfg.perf.tick_allocs_max as u64,
        "в цикле тика {used} аллокаций, бюджет {}",
        cfg.perf.tick_allocs_max
    );
}

/// Сам счётчик работает: иначе тест выше ничего не доказывает.
#[test]
fn counter_detects_allocation() {
    let before = allocs();
    let v: Vec<u8> = Vec::with_capacity(64);
    std::hint::black_box(&v);
    assert!(allocs() > before);
}

#[test]
fn scenario_tick_does_not_allocate_after_warmup() {
    use fsim_core::positioning::team::TeamSetup;
    use fsim_core::scenario_sim::ScenarioSim;
    let cfg = fsim_core::embedded::config().unwrap();
    let sc = &cfg.scenarios["possession_flips"];
    let teams = [0, 1].map(|k| {
        TeamSetup::uniform(
            &cfg,
            &sc.formations[k],
            &cfg.styles[&sc.styles[k]],
            cfg.sandbox.uniform_attr,
        )
        .unwrap()
    });
    let mut sim = ScenarioSim::new(7, &cfg, sc, teams, NoopSink);
    sim.run(100);
    let before = allocs();
    sim.run(600);
    let used = allocs() - before;
    assert!(
        used <= cfg.perf.tick_allocs_max as u64,
        "в тике сценария {used} аллокаций"
    );
}

#[test]
fn live_match_tick_does_not_allocate_after_warmup() {
    use fsim_core::live_match::LiveMatch;
    use fsim_core::positioning::team::TeamSetup;
    let cfg = fsim_core::embedded::config().unwrap();
    let teams = ["positional", "gegenpress"].map(|n| {
        TeamSetup::uniform(&cfg, "4-3-3", &cfg.styles[n], cfg.sandbox.uniform_attr).unwrap()
    });
    let mut m = LiveMatch::new(3, &cfg, teams, NoopSink);
    m.run(600);
    let before = allocs();
    m.run(6000);
    let used = allocs() - before;
    assert!(
        used <= cfg.perf.tick_allocs_max as u64,
        "в тике матча {used} аллокаций"
    );
}
