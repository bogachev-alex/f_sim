//! Ядро движка: мир, физика, восприятие, позиционирование, решения, правила.
//! На M0 здесь каркас: конфиги, потоки PRNG, SoA-мир, `FrameSink`, заглушка матча.

pub mod arrival;
pub mod attrs;
pub mod config;
pub mod curves;
pub mod data;
pub mod decision;
pub mod defense;
pub mod embedded;
pub mod live_match;
pub mod match_loop;
pub mod metrics;
pub mod physics;
pub mod pitch_control;
pub mod positioning;
pub mod rng;
pub mod scenario_sim;
pub mod sink;
pub mod skills;
pub mod team_play;
pub mod value;
pub mod world;
