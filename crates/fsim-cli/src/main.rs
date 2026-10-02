use clap::{Parser, Subcommand};
use fsim_core::config::Config;
use fsim_core::live_match::LiveMatch;
use fsim_core::match_loop::run_sandbox;
use fsim_core::metrics::shape_metrics;
use fsim_core::positioning::team::TeamSetup;
use fsim_core::scenario_sim::{run_scenario, ScenarioSim};
use fsim_core::sink::NoopSink;
use fsim_gen::{generate_equal_pair, GenConfig};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "fsim", about = "Симулятор футбольного матча")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Загрузить и проверить все конфиги.
    CheckConfig {
        #[arg(long, default_value = "config")]
        dir: PathBuf,
    },
    /// Хэш состояния песочницы: для сверки нативной сборки и WASM.
    Determinism {
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 2000)]
        ticks: u32,
        /// Сценарий вместо песочницы.
        #[arg(long)]
        scenario: Option<String>,
        /// Матч positional против gegenpress (4-3-3) вместо песочницы.
        #[arg(long)]
        game: bool,
        #[arg(long, default_value = "config")]
        dir: PathBuf,
    },
    /// Форма команд в сценарии: линия, глубина и ширина блока по фазам.
    Shape {
        #[arg(long, default_value = "shape_hold")]
        scenario: String,
        #[arg(long)]
        style_a: Option<String>,
        #[arg(long)]
        style_b: Option<String>,
        #[arg(long)]
        formation_a: Option<String>,
        #[arg(long)]
        formation_b: Option<String>,
        /// Через сколько секунд снимать метрики (по умолчанию конец сценария).
        #[arg(long)]
        at: Option<f32>,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value = "config")]
        dir: PathBuf,
    },
    /// Сыграть матч (M4: без ударов) и вывести статистику владения и передач.
    Match {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value = "positional")]
        style_a: String,
        #[arg(long, default_value = "gegenpress")]
        style_b: String,
        #[arg(long, default_value = "4-3-3")]
        formation_a: String,
        #[arg(long, default_value = "4-3-3")]
        formation_b: String,
        /// Минут матча.
        #[arg(long, default_value_t = 10.0)]
        minutes: f32,
        #[arg(long, default_value = "config")]
        dir: PathBuf,
    },
    /// Сгенерировать пару одинаковых составов (A и B) в `data/squads/`.
    GenSquads {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value = "4-3-3")]
        formation: String,
        /// Общее качество 0–1; по умолчанию из `generator.json`.
        #[arg(long)]
        quality: Option<f32>,
        #[arg(long, default_value = "config")]
        dir: PathBuf,
        #[arg(long, default_value = "data/squads")]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.cmd {
        Cmd::CheckConfig { dir } => {
            let c = Config::load_dir(&dir)?;
            println!(
                "конфиги в порядке: {} атрибутов, {} композитов",
                c.attributes.all().count(),
                c.composites.0.len()
            );
        }
        Cmd::Determinism {
            seed,
            ticks,
            scenario,
            game,
            dir,
        } => {
            let cfg = Config::load_dir(&dir)?;
            let h = if game {
                fsim_core::live_match::run_match(
                    seed,
                    ticks,
                    &cfg,
                    ["positional", "gegenpress"],
                    ["4-3-3", "4-3-3"],
                )?
            } else {
                match scenario {
                    Some(name) => run_scenario(seed, ticks, &cfg, &name)?,
                    None => run_sandbox(seed, ticks, &cfg),
                }
            };
            println!("{h:016x}");
        }
        Cmd::Shape {
            scenario,
            style_a,
            style_b,
            formation_a,
            formation_b,
            at,
            seed,
            dir,
        } => {
            let cfg = Config::load_dir(&dir)?;
            let sc = cfg.scenarios.get(&scenario).ok_or("сценарий не найден")?;
            let styles = [
                style_a.unwrap_or(sc.styles[0].clone()),
                style_b.unwrap_or(sc.styles[1].clone()),
            ];
            let forms = [
                formation_a.unwrap_or(sc.formations[0].clone()),
                formation_b.unwrap_or(sc.formations[1].clone()),
            ];
            let mut setups = Vec::new();
            for k in 0..2 {
                let style = cfg.styles.get(&styles[k]).ok_or("стиль не найден")?;
                setups.push(TeamSetup::uniform(
                    &cfg,
                    &forms[k],
                    style,
                    cfg.sandbox.uniform_attr,
                )?);
            }
            let [a, b]: [TeamSetup; 2] = setups.try_into().map_err(|_| "две команды")?;
            let mut sim = ScenarioSim::new(seed, &cfg, sc, [a, b], NoopSink);
            let secs = at.unwrap_or(sc.duration_s).min(sc.duration_s);
            sim.run((secs * cfg.physics.time.tick_hz as f32) as u32);
            let half = cfg.physics.pitch.length_m * 0.5;
            println!("сценарий {scenario}, t = {secs:.0} с");
            for k in 0..2 {
                let m = shape_metrics(&sim.world, sim.team(k), k, half);
                let w = sim.world.teams[k].attack_w;
                println!(
                    "{} {:<17} {:<7} фаза атаки {:.2} | линия {:5.1} м, перед {:5.1} м, глубина {:5.1} м, длина {:5.1} м, ширина {:5.1} м",
                    ["A", "B"][k], styles[k], forms[k], w, m.back_x, m.front_x, m.depth, m.length, m.width
                );
            }
        }
        Cmd::Match {
            seed,
            style_a,
            style_b,
            formation_a,
            formation_b,
            minutes,
            dir,
        } => {
            let cfg = Config::load_dir(&dir)?;
            let mut teams = Vec::new();
            for (st, fm) in [(&style_a, &formation_a), (&style_b, &formation_b)] {
                let style = cfg.styles.get(st).ok_or("стиль не найден")?;
                teams.push(TeamSetup::uniform(
                    &cfg,
                    fm,
                    style,
                    cfg.sandbox.uniform_attr,
                )?);
            }
            let [a, b]: [TeamSetup; 2] = teams.try_into().map_err(|_| "две команды")?;
            let mut m = LiveMatch::new(seed, &cfg, [a, b], NoopSink);
            let hz = cfg.physics.time.tick_hz as f32;
            m.run((minutes * 60.0 * hz) as u32);
            let s = m.stats;
            println!("матч {style_a} {formation_a} против {style_b} {formation_b}, {minutes} мин, seed {seed}");
            for k in 0..2 {
                let acc = if s.passes[k] > 0 {
                    100.0 * s.passes_ok[k] as f32 / s.passes[k] as f32
                } else {
                    0.0
                };
                println!(
                    "{}: передач {} (точных {:.0}%), перехватов {}, отборов {} (неудачных {}), владений {}",
                    ["A", "B"][k], s.passes[k], acc, s.interceptions[k], s.tackles_won[k], s.tackles_failed[k], s.possessions[k]
                );
            }
            println!(
                "выходов мяча за линии {}, голов {}:{}",
                s.out_of_play, s.goals[0], s.goals[1]
            );
            for k in 0..2 {
                println!(
                    "{}: ударов {} (в створ {}, блок {}, сейвов вратаря {}), xG {:.2}",
                    ["A", "B"][k],
                    s.shots[k],
                    s.shots_on_target[k],
                    s.blocks[1 - k],
                    s.saves[k],
                    s.xg[k]
                );
            }
            let cal: Vec<String> = s
                .pass_calibration
                .iter()
                .enumerate()
                .filter(|(_, c)| c[0] > 0)
                .map(|(i, c)| {
                    format!(
                        "{}–{}%: {}/{} ({:.0}%)",
                        i * 10,
                        i * 10 + 10,
                        c[1],
                        c[0],
                        100.0 * c[1] as f32 / c[0] as f32
                    )
                })
                .collect();
            println!(
                "калибровка паса (ожидание: принято/попыток): {}",
                cal.join(", ")
            );
            let d = s.decisions;
            println!(
                "решения: пас в ноги {}, в пространство {}, за спину {}, длинный {}, ведение {}, укрывание {}, вынос {}, удар {}",
                d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
            );
            for (i, name) in [
                "в ноги",
                "в пространство",
                "за спину",
                "длинный",
                "",
                "",
                "вынос",
            ]
            .iter()
            .enumerate()
            {
                if s.pass_attempts[i] > 0 {
                    println!(
                        "  {name}: попыток {}, принято {} ({:.0}%), ожидалось {:.0}%",
                        s.pass_attempts[i],
                        s.pass_completed[i],
                        100.0 * s.pass_completed[i] as f32 / s.pass_attempts[i] as f32,
                        100.0 * s.pass_expected[i] / s.pass_attempts[i] as f32
                    );
                    let f = s.pass_fail[i];
                    println!("    провалы: перехват {}, плохой приём партнёра {}, плохой приём соперника {}, за линию {}", f[0], f[1], f[2], f[3]);
                }
            }
        }
        Cmd::GenSquads {
            seed,
            formation,
            quality,
            dir,
            out,
        } => {
            let cfg = Config::load_dir(&dir)?;
            let gen = GenConfig::from_json(&std::fs::read_to_string(dir.join("generator.json"))?)?;
            let (a, b) = generate_equal_pair(&gen, &cfg.formations, seed, &formation, quality)?;
            std::fs::create_dir_all(&out)?;
            for squad in [&a, &b] {
                let path = out.join(format!("{}_{}_{}.json", squad.name, formation, seed));
                std::fs::write(&path, serde_json::to_string_pretty(squad)?)?;
                println!("{}", path.display());
            }
        }
    }
    Ok(())
}
