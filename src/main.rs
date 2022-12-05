use indicatif::*;
use is_terminal::*;
use serde::*;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::*,
    time::{Duration, Instant},
};

use structopt::*;

mod cache;

mod multi_runner;
use multi_runner::*;

mod targets;

#[derive(StructOpt)]
struct Options {
    #[structopt(long, default_value = "./build/config.toml")]
    config_file: PathBuf,

    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
pub enum Command {
    CacheLoad {
        from: PathBuf,
    },
    CacheSave {
        to: PathBuf,
    },

    // TODO(shelbyd): Allow multiple actions.
    #[structopt(flatten)]
    Action(Action),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, StructOpt)]
pub enum Action {
    Test,
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Test => write!(f, "test"),
        }
    }
}

#[derive(Deserialize, Default)]
struct Config {
    skip: HashSet<String>,
}

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();

    let config = if let Ok(file) = std::fs::read(&options.config_file) {
        toml::from_slice(&file)?
    } else {
        Config::default()
    };

    match options.command {
        Command::Action(action) => {
            let targets = targets::targets()?
                .into_iter()
                .filter(|t| !config.skip.contains(&t.to_string()))
                .collect::<Vec<_>>();

            let progress: Box<dyn ProgressListener> =
                if std::env::var("CI") == Ok(String::from("true")) {
                    Box::new(ContinuousIntegrationProgress::new(targets.len()))
                } else if std::io::stderr().is_terminal() {
                    Box::new(TermProgress::new())
                } else {
                    Box::new(NullProgressListener)
                };
            let mut runner = ParRunner::new(progress);

            for target in targets {
                if config.skip.contains(&target.to_string()) {
                    continue;
                }

                runner
                    .run(&format!("{action} {target}"), move || match action {
                        Action::Test => target.perform_test(),
                    })
                    .map_err(|(id, err)| err.context(id))?;
            }
            runner.into_wait().map_err(|(id, err)| err.context(id))?;
        }

        Command::CacheLoad { from } => cache::load(from)?,
        Command::CacheSave { to } => cache::save(to)?,
    }

    Ok(())
}

struct TermProgress {
    multi: MultiProgress,
    bars: Vec<(ProgressBar, Option<String>)>,
}

impl TermProgress {
    fn new() -> Self {
        TermProgress {
            multi: MultiProgress::new(),
            bars: Default::default(),
        }
    }
}

impl Drop for TermProgress {
    fn drop(&mut self) {
        for (bar, _) in &self.bars {
            bar.finish_and_clear();
        }
    }
}

impl ProgressListener for TermProgress {
    fn on_start(&mut self, name: &str) {
        for (bar, running) in &mut self.bars {
            if running.is_some() {
                continue;
            }

            bar.set_message(name.to_string());
            bar.reset();
            *running = Some(name.to_string());
            return;
        }

        let p = self.multi.add(ProgressBar::new_spinner());
        p.set_message(name.to_string());
        p.enable_steady_tick(Duration::from_millis(50));

        self.bars.push((p, Some(name.to_string())));
    }

    fn on_finish(&mut self, name: &str) {
        let (bar, running) = self
            .bars
            .iter_mut()
            .find(|(_, r)| r.as_ref() == Some(&name.to_string()))
            .expect("called on_finish without on_start");

        *running = None;
        bar.set_message("");
        bar.finish();
    }
}

#[derive(Default)]
struct ContinuousIntegrationProgress {
    total: usize,
    running: HashMap<String, Instant>,
    finished: HashMap<String, Duration>,
}

impl ContinuousIntegrationProgress {
    fn new(total: usize) -> Self {
        eprintln!("Running {total} tasks");

        ContinuousIntegrationProgress {
            total,
            running: Default::default(),
            finished: Default::default(),
        }
    }

    fn log_status(&self) {
        eprintln!(
            "Running {}, finished {} / {}",
            self.running.len(),
            self.finished.len(),
            self.total
        );
        for (name, started) in &self.running {
            eprintln!(
                "  {name}: {}",
                humantime::format_duration(started.elapsed())
            );
        }
    }
}

impl ProgressListener for ContinuousIntegrationProgress {
    fn on_start(&mut self, name: &str) {
        eprintln!("Starting {name}");
        self.running.insert(name.to_string(), Instant::now());

        self.log_status();
    }

    fn on_finish(&mut self, name: &str) {
        let started_at = self
            .running
            .remove(name)
            .expect("called on_finish without on_start");
        let took = started_at.elapsed();
        eprintln!("Finished {name} in {}", humantime::format_duration(took));

        self.finished.insert(name.to_string(), took);

        self.log_status();
    }
}

impl Drop for ContinuousIntegrationProgress {
    fn drop(&mut self) {
        eprintln!("Runtime report:");

        let mut sorted_order = self.finished.drain().collect::<Vec<_>>();
        sorted_order.sort_by_key(|(_, d)| *d);

        for (name, dur) in sorted_order {
            eprintln!("  {}: {name}", humantime::format_duration(dur));
        }
    }
}
