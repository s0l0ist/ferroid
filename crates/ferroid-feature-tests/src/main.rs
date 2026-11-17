mod cli;
mod runner;
mod state;

use anyhow::Result;
use clap::Parser;
use cli::{Action, Cli};
use state::{Status, Store};
use std::{io::Write, path::Path, process::ExitCode};

#[tokio::main]
async fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let path = root.join(cli.state);
    let mut store = Store::open(path)?;
    let selected = match cli.command {
        Action::Clean => {
            store.clean()?;
            return Ok(ExitCode::SUCCESS);
        }
        Action::Run(matrix) => {
            println!("Enumerating Cargo commands...");
            std::io::stdout().flush()?;
            let checkpoint =
                runner::enumerate(&root, matrix.hack_args(), matrix.toolchain.as_deref()).await?;
            store.create(checkpoint, matrix.overwrite)?;
            Status::Pending
        }
        Action::Status => {
            store.load(&root)?;
            store.summary();
            return Ok(if store.has_failures() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            });
        }
        action @ (Action::Resume | Action::RetryFailed) => {
            store.load(&root)?;
            if matches!(action, Action::RetryFailed) {
                Status::Failed
            } else {
                Status::Pending
            }
        }
    };
    runner::Runner::new(
        store,
        selected,
        cli.jobs.map(|jobs| jobs.get()),
        cli.build_jobs.get(),
    )?
    .run()
    .await
}
