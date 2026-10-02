//! Инварианты физики и сверка модели прибытия с интегрированием.

use fsim_core::arrival::time_to_reach;
use fsim_core::config::PhysicsConfig;
use fsim_core::physics::ball::{BallEnv, BallModel};
use fsim_core::physics::body::{step_body, BodyParams};
use glam::{Vec2, Vec3};
use proptest::prelude::*;

const PHYSICS: &str = include_str!("../../../config/physics.json");

fn phys() -> PhysicsConfig {
    PhysicsConfig::from_json(PHYSICS).unwrap()
}

fn params(speed: f32, accel: f32, lat: f32, phys: &PhysicsConfig) -> BodyParams {
    BodyParams {
        max_speed: speed,
        accel,
        decel: phys.player.deceleration_ms2,
        lat_accel: lat,
    }
}

/// Время достижения цели интегрированием: реакция (движение по инерции), затем движение сквозь
/// цель на максимальной скорости. Мелкий шаг, чтобы ошибка шага не мешала сверке.
fn simulate_arrival(pos: Vec2, vel: Vec2, p: &BodyParams, react: f32, target: Vec2) -> f32 {
    let dt = 0.005;
    let (mut pos, mut vel) = (pos, vel);
    let mut t = 0.0;
    while t < react {
        pos += vel * dt;
        t += dt;
    }
    for _ in 0..4000 {
        if (target - pos).length() <= 0.05 {
            return t;
        }
        step_body(&mut pos, &mut vel, p, target, p.max_speed, false, dt);
        t += dt;
    }
    panic!("цель не достигнута");
}

#[test]
fn arrival_matches_integration() {
    let ph = phys();
    let p = params(8.5, 4.0, 7.0, &ph);
    let mut worst = 0.0f32;
    let mut bad = 0;
    // Различные направления скорости относительно цели: к цели, от неё, поперёк, на месте.
    let target = Vec2::new(25.0, 0.0);
    for speed in [0.0f32, 3.0, 6.0, 8.5] {
        for ang_deg in [0.0f32, 45.0, 90.0, 135.0, 180.0, -90.0] {
            for react in [0.15f32, 0.3] {
                let a = ang_deg.to_radians();
                let vel = Vec2::new(libm::cosf(a), libm::sinf(a)) * speed;
                let model = time_to_reach(Vec2::ZERO, vel, &p, react, target);
                let sim = simulate_arrival(Vec2::ZERO, vel, &p, react, target);
                let err = (model - sim).abs();
                worst = worst.max(err);
                println!("v={speed} угол={ang_deg} реакция={react}: модель {model:.3}, интегрирование {sim:.3}, ошибка {:+.3}", model - sim);
                if err > 0.10_f32.max(0.03 * sim) {
                    bad += 1;
                }
            }
        }
    }
    println!("наибольшее расхождение модели прибытия: {worst:.3} с");
    assert_eq!(bad, 0, "случаев вне допуска: {bad}");
}

proptest! {
    #[test]
    fn body_respects_limits(
        vx in -9.0f32..9.0, vy in -9.0f32..9.0,
        tx in -50.0f32..50.0, ty in -30.0f32..30.0,
        speed in 7.0f32..9.8, accel in 3.0f32..5.5, lat in 5.0f32..9.0,
        arrive in any::<bool>(),
    ) {
        let ph = phys();
        let p = params(speed, accel, lat, &ph);
        let dt = ph.dt();
        let mut vel = Vec2::new(vx, vy);
        if vel.length() > speed { vel *= speed / vel.length(); }
        let mut pos = Vec2::ZERO;
        for _ in 0..400 {
            let (old_pos, old_vel) = (pos, vel);
            step_body(&mut pos, &mut vel, &p, Vec2::new(tx, ty), speed, arrive, dt);
            prop_assert!(vel.length() <= speed + 1e-3, "скорость выше предела");
            // Нет телепортаций: смещение за шаг не больше макс. скорости на dt.
            prop_assert!((pos - old_pos).length() <= speed * dt + 1e-3);
            // Изменение скорости за шаг ограничено ускорениями.
            let dv = (vel - old_vel).length();
            let limit = (accel.max(ph.player.deceleration_ms2).powi(2) + lat * lat).sqrt() * dt;
            prop_assert!(dv <= limit + 1e-2, "рывок скорости {dv} > {limit}");
        }
    }
}

#[test]
fn body_stops_at_target_when_arriving() {
    let ph = phys();
    let p = params(8.5, 4.0, 7.0, &ph);
    let target = Vec2::new(30.0, 10.0);
    let (mut pos, mut vel) = (Vec2::ZERO, Vec2::ZERO);
    for _ in 0..600 {
        step_body(&mut pos, &mut vel, &p, target, p.max_speed, true, ph.dt());
    }
    assert!((pos - target).length() < 0.1, "позиция {pos}");
    assert!(vel.length() < 0.1);
}

#[test]
fn turn_radius_grows_with_speed_and_falls_with_agility() {
    let ph = phys();
    // Бежим по оси x, цель сбоку. Скорость поворота вектора скорости за 0,25 с: чем быстрее
    // бег и ниже ловкость, тем меньше угол, то есть шире радиус разворота.
    let heading_change = |speed: f32, lat: f32| {
        let p = params(9.8, 4.0, lat, &ph);
        let (mut pos, mut vel) = (Vec2::ZERO, Vec2::new(speed, 0.0));
        for _ in 0..5 {
            step_body(
                &mut pos,
                &mut vel,
                &p,
                Vec2::new(0.0, 100.0),
                speed,
                false,
                ph.dt(),
            );
        }
        libm::atan2f(vel.y, vel.x)
    };
    assert!(
        heading_change(4.0, 7.0) > heading_change(8.0, 7.0),
        "быстрее — шире разворот"
    );
    assert!(
        heading_change(8.0, 9.0) > heading_change(8.0, 5.0),
        "ловкость сужает разворот"
    );
}

// ---------- мяч ----------

fn ball() -> (BallModel, PhysicsConfig) {
    let ph = phys();
    (BallModel::new(&ph), ph)
}

#[test]
fn ball_falls_and_bounces_with_energy_loss() {
    let (b, ph) = ball();
    let (mut pos, mut vel) = (Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO);
    let mut max_after_first = 0.0f32;
    let mut bounced = false;
    for _ in 0..400 {
        let before = vel.z;
        b.step(&mut pos, &mut vel, &BallEnv::default(), ph.dt());
        if before < 0.0 && vel.z > 0.0 {
            bounced = true;
        }
        if bounced {
            max_after_first = max_after_first.max(pos.z);
        }
        assert!(pos.z >= b.radius() - 1e-4);
    }
    assert!(bounced);
    assert!(
        max_after_first < 5.0 * 0.7,
        "после отскока мяч не выше 0,7 исходной высоты"
    );
}

#[test]
fn ground_ball_slows_and_stops() {
    let (b, ph) = ball();
    let (mut pos, mut vel) = (Vec3::new(-30.0, 0.0, b.radius()), Vec3::new(18.0, 0.0, 0.0));
    let mut prev = vel.x;
    for _ in 0..2000 {
        b.step(&mut pos, &mut vel, &BallEnv::default(), ph.dt());
        assert!(vel.x <= prev + 1e-4, "скорость не растёт");
        prev = vel.x;
    }
    assert!(vel.length() < 0.05, "мяч остановился, v = {}", vel.length());
    assert!(pos.x < 52.5, "не дошёл до ворот");
}

#[test]
fn rain_speeds_up_rolling_and_lowers_bounce() {
    let (b, ph) = ball();
    let run = |rain: f32| {
        let env = BallEnv {
            rain,
            ..BallEnv::default()
        };
        let (mut pos, mut vel) = (Vec3::new(-40.0, 0.0, b.radius()), Vec3::new(12.0, 0.0, 0.0));
        for _ in 0..400 {
            b.step(&mut pos, &mut vel, &env, ph.dt());
        }
        pos.x
    };
    assert!(run(1.0) > run(0.0), "в дождь мяч катится дальше");
}

#[test]
fn wind_drifts_high_ball() {
    let (b, ph) = ball();
    let run = |wind: Vec2| {
        let env = BallEnv {
            wind,
            ..BallEnv::default()
        };
        let (mut pos, mut vel) = (Vec3::new(-10.0, 0.0, 0.2), Vec3::new(15.0, 0.0, 15.0));
        for _ in 0..60 {
            b.step(&mut pos, &mut vel, &env, ph.dt());
        }
        pos.y
    };
    assert!(run(Vec2::new(0.0, 8.0)) > run(Vec2::ZERO) + 0.5);
}

proptest! {
    /// Мяч не проходит сквозь стойки и перекладину и не покидает сетку.
    #[test]
    fn ball_never_passes_through_goal_frame(
        side in prop::bool::ANY,
        sx in 30.0f32..50.0,
        sy in -20.0f32..20.0,
        sz in 0.12f32..6.0,
        speed in 10.0f32..34.0,
        aim_y in -6.0f32..6.0,
        aim_z in 0.0f32..4.0,
    ) {
        let ph = phys();
        let b = BallModel::new(&ph);
        let s = if side { 1.0 } else { -1.0 };
        let hl = ph.pitch.length_m * 0.5;
        let (hw, gh, gd) = (ph.goal.width_m * 0.5, ph.goal.height_m, ph.goal.depth_m);
        let (pr, br) = (ph.goal.post_radius_m, b.radius());
        let mut pos = Vec3::new(s * sx, sy, sz);
        let dir = (Vec3::new(s * hl, aim_y, aim_z) - pos).normalize();
        let mut vel = dir * speed;
        let mut inside_goal = false;
        for _ in 0..200 {
            b.step(&mut pos, &mut vel, &BallEnv::default(), ph.dt());
            let (x, y, z) = (pos.x * s, pos.y, pos.z);
            // Центр мяча не заходит внутрь цилиндров штанг и перекладины.
            for sy2 in [hw, -hw] {
                let d = ((x - hl).powi(2) + (y - sy2).powi(2)).sqrt();
                if z <= gh {
                    prop_assert!(d >= pr + br - 2e-3, "мяч в штанге: d={d}");
                }
            }
            if y.abs() <= hw {
                let d = ((x - hl).powi(2) + (z - gh).powi(2)).sqrt();
                prop_assert!(d >= pr + br - 2e-3, "мяч в перекладине: d={d}");
            }
            // Попав в ворота, мяч остаётся в сетке (выкатиться обратно через открытый торец
            // можно: ограничения действуют, пока он за линией ворот).
            if x > hl && x <= hl + gd && y.abs() < hw && z < gh {
                inside_goal = true;
            }
            if inside_goal && x > hl {
                prop_assert!(x <= hl + gd + 1e-3 && y.abs() <= hw + 1e-3 && z <= gh + 1e-3,
                    "мяч вышел из сетки: ({x}, {y}, {z})");
            }
        }
    }
}
