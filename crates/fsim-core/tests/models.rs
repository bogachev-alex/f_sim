//! Модели решений: xT, xG, качение мяча, контроль пространства.

use fsim_core::config::Config;
use fsim_core::decision::{ball_flight, goal_angle, xg, Ctx};
use fsim_core::embedded;
use fsim_core::physics::ball::{BallEnv, BallModel};
use fsim_core::pitch_control::{control_at, ControlGrid};
use fsim_core::positioning::team::TeamSetup;
use fsim_core::value::Xt;
use fsim_core::world::{World, PLAYERS};
use glam::{Vec2, Vec3};

fn cfg() -> Config {
    embedded::config().unwrap()
}

fn xt(c: &Config) -> Xt {
    Xt::new(
        c.xt.clone(),
        c.physics.pitch.length_m,
        c.physics.pitch.width_m,
    )
}

#[test]
fn xt_grows_toward_the_opponent_goal_and_is_bounded() {
    let c = cfg();
    let x = xt(&c);
    let l = c.physics.pitch.length_m;
    let mut prev = 0.0;
    for k in 1..=20 {
        let v = x.at(Vec2::new(l * k as f32 / 20.0 - 1.0, 0.0));
        assert!(v >= prev - 1e-6, "xT не растёт к воротам на шаге {k}");
        prev = v;
    }
    assert!(
        x.at(Vec2::new(l - 8.0, 0.0)) > 0.15,
        "у ворот ценность высокая"
    );
    assert!(x.at(Vec2::new(8.0, 0.0)) < 0.005, "у своих ворот низкая");
    // Центр ценнее фланга на одной глубине у ворот.
    assert!(x.at(Vec2::new(l - 12.0, 0.0)) > x.at(Vec2::new(l - 12.0, 30.0)));
    // Ценность для соперника зеркальна.
    let p = Vec2::new(70.0, 10.0);
    assert!((x.for_opponent(p) - x.at(Vec2::new(l - 70.0, -10.0))).abs() < 1e-6);
    // Непрерывность: малый сдвиг даёт малое изменение.
    let a = x.at(Vec2::new(60.0, 5.0));
    let b = x.at(Vec2::new(60.01, 5.0));
    assert!((a - b).abs() < 1e-3);
}

fn with_ctx<R>(c: &Config, f: impl FnOnce(&Ctx) -> R) -> R {
    let team =
        TeamSetup::uniform(c, "4-3-3", &c.styles["balanced"], c.sandbox.uniform_attr).unwrap();
    let xt = xt(c);
    let z = [Vec2::ZERO; 11];
    let ctx = Ctx {
        cfg: &c.decision,
        phys: &c.physics,
        xt: &xt,
        me: 6,
        mates_pos: &z,
        mates_vel: &z,
        opp_pos: &z,
        opp_vel: &z,
        team: &team,
        opp: &team,
        held_s: 0.0,
    };
    f(&ctx)
}

#[test]
fn xg_depends_on_distance_angle_pressure_and_skill() {
    let c = cfg();
    let l = c.physics.pitch.length_m;
    with_ctx(&c, |ctx| {
        let central = |d: f32| xg(ctx, Vec2::new(l - d, 0.0), 0.0, 0.5);
        // Ориентиры логистической модели: с 11 м по центру около трети, с 30 м единицы процентов.
        assert!(
            (0.28..0.42).contains(&central(11.0)),
            "xG с 11 м: {}",
            central(11.0)
        );
        assert!(central(30.0) < 0.05, "xG с 30 м: {}", central(30.0));
        let mut prev = 1.0;
        for d in [6.0, 9.0, 12.0, 16.0, 20.0, 25.0, 30.0] {
            assert!(central(d) < prev, "xG растёт с дистанцией на {d} м");
            prev = central(d);
        }
        // Острый угол хуже прямого на той же дистанции.
        let wide = xg(ctx, Vec2::new(l - 8.0, 20.0), 0.0, 0.5);
        let straight = xg(ctx, Vec2::new(l - 21.5, 0.0), 0.0, 0.5);
        assert!(
            wide < straight,
            "острый угол {wide} против прямого {straight}"
        );
        assert!(
            xg(ctx, Vec2::new(l - 11.0, 0.0), 1.0, 0.5) < central(11.0),
            "давление снижает xG"
        );
        assert!(
            xg(ctx, Vec2::new(l - 11.0, 0.0), 0.0, 0.9) > central(11.0),
            "навык повышает xG"
        );
        // Вероятность остаётся вероятностью.
        for d in [1.0, 5.0, 40.0] {
            let v = central(d);
            assert!((0.0..=1.0).contains(&v));
        }
    });
}

#[test]
fn goal_angle_is_symmetric_and_shrinks_with_distance() {
    let c = cfg();
    let (l, gw) = (c.physics.pitch.length_m, c.physics.goal.width_m);
    let a = |x: f32, y: f32| goal_angle(Vec2::new(x, y), l, gw);
    assert!((a(l - 10.0, 5.0) - a(l - 10.0, -5.0)).abs() < 1e-5);
    assert!(a(l - 10.0, 0.0) > a(l - 20.0, 0.0));
    // На линии ворот между штангами угол полный.
    assert!(a(l - 0.5, 0.0) > 1.5);
}

#[test]
fn ball_flight_matches_ball_physics_integration() {
    let c = cfg();
    let model = BallModel::new(&c.physics);
    let dt = c.physics.dt();
    for v0 in [12.0f32, 18.0, 24.0] {
        // Интегрируем мяч по земле до пройденных 30 м.
        let (mut pos, mut vel) = (
            Vec3::new(-40.0, 0.0, c.physics.ball.radius_m),
            Vec3::new(v0, 0.0, 0.0),
        );
        let start = pos.x;
        let mut t = 0.0;
        while pos.x - start < 30.0 && t < 20.0 {
            model.step(&mut pos, &mut vel, &BallEnv::default(), dt);
            t += dt;
        }
        let prof = ball_flight(&c.physics, v0, 30.0, 8);
        let err = (prof.t_end - t).abs();
        assert!(
            err < 0.12 || err < 0.04 * t,
            "v0={v0}: профиль {} с против интегрирования {t} с",
            prof.t_end
        );
        // Скорость у цели тоже совпадает.
        assert!(
            (prof.v_arrive - vel.x).abs() < 0.8,
            "v0={v0}: скорость {} против {}",
            prof.v_arrive,
            vel.x
        );
        // Времена по точкам возрастают.
        assert!(prof.times[..8].windows(2).all(|w| w[1] > w[0]));
    }
}

#[test]
fn ball_flight_reports_a_stopping_ball() {
    let c = cfg();
    // Слабый удар на большое расстояние: мяч остановится раньше конца пути.
    let prof = ball_flight(&c.physics, 6.0, 60.0, 8);
    assert_eq!(prof.t_end, f32::MAX);
    assert_eq!(prof.v_arrive, 0.0);
}

fn world_with(c: &Config, positions: &[(usize, f32, f32)]) -> World {
    let team =
        TeamSetup::uniform(c, "4-3-3", &c.styles["balanced"], c.sandbox.uniform_attr).unwrap();
    let mut body = [team.body[0]; PLAYERS];
    for (k, b) in body.iter_mut().enumerate() {
        *b = team.body[k % 11];
    }
    let mut w = World::new(body);
    // Все далеко за пределами интереса, затем явные позиции.
    for n in 0..PLAYERS {
        w.players.pos_x[n] = if n < 11 { -50.0 } else { 50.0 };
        w.players.pos_y[n] = (n % 11) as f32;
    }
    for &(n, x, y) in positions {
        w.players.pos_x[n] = x;
        w.players.pos_y[n] = y;
    }
    w
}

#[test]
fn pitch_control_favors_the_nearer_team_and_is_balanced_at_the_midpoint() {
    let c = cfg();
    let w = world_with(&c, &[(5, -10.0, 0.0), (16, 10.0, 0.0)]);
    let ctl = |p: Vec2, team: usize| control_at(&w.players, &w.body, 0.2, 0.5, p, team);
    assert!(
        (ctl(Vec2::new(0.0, 0.0), 0) - 0.5).abs() < 0.05,
        "в середине между двумя равными игроками"
    );
    assert!(
        ctl(Vec2::new(-8.0, 0.0), 0) > 0.8,
        "около своего игрока контроль высокий"
    );
    assert!(ctl(Vec2::new(8.0, 0.0), 0) < 0.2);
    // Контроль двух команд в сумме даёт единицу.
    let p = Vec2::new(-3.0, 4.0);
    assert!((ctl(p, 0) + ctl(p, 1) - 1.0).abs() < 1e-5);
}

#[test]
fn control_grid_matches_exact_control_at_cell_centres() {
    let c = cfg();
    let w = world_with(&c, &[(5, -10.0, 5.0), (16, 12.0, -4.0), (2, -25.0, 0.0)]);
    let mut g = ControlGrid::new(c.physics.pitch.length_m, c.physics.pitch.width_m);
    g.update(&w.players, &w.body, 0.2, 0.5);
    for (row, col) in [(3, 4), (7, 10), (10, 16)] {
        let p = g.cell_center(row, col);
        let exact = control_at(&w.players, &w.body, 0.2, 0.5, p, 0);
        assert!((g.cells[row][col] - exact).abs() < 1e-5);
        // Интерполяция в центре клетки совпадает с её значением.
        assert!(
            (g.sample(p) - exact).abs() < 0.02,
            "выборка {} против {exact}",
            g.sample(p)
        );
    }
}
