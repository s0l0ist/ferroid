use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{num::NonZeroUsize, path::PathBuf};

#[derive(Parser)]
#[command(about = "Check feature combinations with persistent, resumable results")]
pub struct Cli {
    /// Checkpoint file. Use separate files for independent matrices.
    #[arg(
        long,
        global = true,
        default_value = "crates/ferroid-feature-tests/state/state.json"
    )]
    pub state: PathBuf,
    /// Concurrent Cargo processes; defaults to available CPUs.
    #[arg(long, global = true)]
    pub jobs: Option<NonZeroUsize>,
    /// Compiler jobs per Cargo process.
    #[arg(long, global = true, default_value = "1")]
    pub build_jobs: NonZeroUsize,
    #[command(subcommand)]
    pub command: Action,
}

#[derive(Subcommand)]
pub enum Action {
    /// Enumerate a new matrix and run it.
    Run(Matrix),
    /// Run pending or interrupted cases, keeping previous results.
    Resume,
    /// Rerun only failed cases, keeping passing and pending cases untouched.
    RetryFailed,
    /// Show saved results and reproduction commands for failures.
    Status,
    /// Remove this checkpoint and its logs, keeping worker build caches.
    Clean,
}

#[derive(Clone, ValueEnum)]
enum Mode {
    Check,
    Test,
}

#[derive(Args)]
pub struct Matrix {
    #[arg(long, value_enum, default_value = "test")]
    mode: Mode,
    #[arg(short, long, default_value = "ferroid")]
    package: String,
    /// Test single features instead of the feature powerset.
    #[arg(long)]
    each_feature: bool,
    /// Limit the number of simultaneously selected features.
    #[arg(long, conflicts_with = "each_feature")]
    depth: Option<NonZeroUsize>,
    /// Always enable these features.
    #[arg(long)]
    features: Option<String>,
    /// Require one of these features in powerset mode. Empty string disables it.
    #[arg(long, default_value = "snowflake,ulid")]
    at_least_one_of: String,
    #[arg(long)]
    exclude_features: Option<String>,
    /// Check/test every target instead of just the library.
    #[arg(long)]
    all_targets: bool,
    /// Rustup toolchain to use for enumeration and execution.
    #[arg(long)]
    pub toolchain: Option<String>,
    /// Replace an existing checkpoint with a new matrix.
    #[arg(long)]
    pub overwrite: bool,
    /// Additional Cargo arguments after --.
    #[arg(last = true)]
    cargo_args: Vec<String>,
}

impl Matrix {
    pub fn hack_args(&self) -> Vec<String> {
        let mut args = vec![
            match self.mode {
                Mode::Check => "check",
                Mode::Test => "test",
            }
            .into(),
            "-p".into(),
            self.package.clone(),
            "--locked".into(),
            if self.all_targets {
                "--all-targets"
            } else {
                "--lib"
            }
            .into(),
            if self.each_feature {
                "--each-feature"
            } else {
                "--feature-powerset"
            }
            .into(),
        ];
        if let Some(features) = &self.features {
            args.extend(["--features".into(), features.clone()]);
        }
        if let Some(features) = &self.exclude_features {
            args.extend(["--exclude-features".into(), features.clone()]);
        }
        if let Some(depth) = self.depth {
            args.extend(["--depth".into(), depth.to_string()]);
        }
        if !self.each_feature && !self.at_least_one_of.is_empty() {
            args.extend(["--at-least-one-of".into(), self.at_least_one_of.clone()]);
        }
        args.extend(self.cargo_args.clone());
        args
    }
}
