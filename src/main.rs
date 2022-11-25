use anyhow::Context;
use std::path::PathBuf;
use structopt::*;

mod target;

#[derive(StructOpt, Debug)]
struct Options {
    /// The root directory to work against. Will be inferred based on the current directory if not
    /// provided.
    #[structopt(long)]
    root: Option<PathBuf>,

    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt, Debug)]
enum Command {
    Run {
        // TODO(shelbyd): Support multiple targets.
        /// Target to run.
        target: target::Target,
    },
}

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();

    let root = match options.root {
        Some(r) => r,
        None => std::env::current_dir()?,
    };

    match options.command {
        Command::Run { target } => {
            let file = root.join(&target.package).join("BUILD");
            let root_build = std::fs::read_to_string(&file).context(format!("reading {file:?}"))?;
            eprintln!("{root_build}");
            todo!();
        }
    }

    Ok(())
}
