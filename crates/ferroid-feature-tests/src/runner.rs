use crate::state::{Case, Checkpoint, Status, Store};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{ExitCode, ExitStatus, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::Mutex, task::JoinSet};

pub async fn enumerate(
    root: &Path,
    args: Vec<String>,
    toolchain: Option<&str>,
) -> Result<Checkpoint> {
    let mut cargo = Command::new("cargo");
    if let Some(toolchain) = toolchain {
        cargo.arg(format!("+{toolchain}"));
    }
    let output = cargo
        .args(["hack", "--print-command-list", "--color", "never"])
        .args(&args)
        .current_dir(root)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not run cargo-hack")?;
    ensure!(
        output.status.success(),
        "cargo-hack failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = String::from_utf8(output.stdout)?;
    let mut cases = Vec::new();
    for line in commands.lines().filter(|line| !line.trim().is_empty()) {
        let mut parsed = shlex::split(line).context("invalid quoting in cargo-hack command")?;
        ensure!(
            parsed.first().map(String::as_str) == Some("cargo"),
            "unexpected command: {line}"
        );
        if let Some(toolchain) = toolchain {
            parsed.insert(1, format!("+{toolchain}"));
        }
        ensure!(
            !parsed
                .iter()
                .any(|arg| arg == "--target-dir" || arg.starts_with("--target-dir=")),
            "target directories are managed by the harness"
        );
        cases.push(Case {
            command: shlex::try_join(parsed.iter().map(String::as_str))?,
            args: parsed[1..].to_vec(),
            status: Status::Pending,
            attempts: 0,
            log: None,
            reason: None,
            warnings: Vec::new(),
        });
    }
    ensure!(!cases.is_empty(), "cargo-hack produced no commands");
    Ok(Checkpoint {
        version: 1,
        root: root.to_owned(),
        enumeration: args,
        cases,
    })
}

pub struct Runner {
    store: Mutex<Store>,
    queues: Vec<Vec<usize>>,
    root: PathBuf,
    targets: PathBuf,
    build_jobs: usize,
    total: usize,
    initial_completed: usize,
    session_total: usize,
}

// A child can receive Ctrl-C before the coordinator observes it.
// Keep its case running until shutdown recovers it as pending.
enum Outcome {
    Complete,
    Interrupted,
}

impl Runner {
    pub fn new(
        mut store: Store,
        selected: Status,
        jobs: Option<usize>,
        build_jobs: usize,
    ) -> Result<Self> {
        let workers = match jobs {
            Some(jobs) => jobs,
            None => std::thread::available_parallelism()?.get(),
        };
        let mut queues = vec![Vec::new(); workers];
        for (index, case) in store.cases().iter().enumerate() {
            if case.status == selected {
                // Use the saved index, so resume/retry filtering preserves affinity.
                queues[index % workers].push(index);
            }
        }
        let total = store.cases().len();
        let session_total = queues.iter().map(Vec::len).sum();
        // Persist queued retries so resume also includes retries not started yet.
        if selected == Status::Failed {
            for &index in queues.iter().flatten() {
                store.pending(index)?;
            }
        }
        let initial_completed = store.counts().completed();
        fs::create_dir_all(store.path.with_extension("logs"))?;
        let root = store.root().to_owned();
        let targets = store.path.with_extension("build");
        println!(
            "Running {session_total} commands with {workers} workers, {build_jobs} compiler jobs each ({initial_completed}/{total} already completed)."
        );
        Ok(Self {
            store: Mutex::new(store),
            queues,
            root,
            targets,
            build_jobs,
            total,
            initial_completed,
            session_total,
        })
    }

    pub async fn run(mut self) -> Result<ExitCode> {
        let queues = std::mem::take(&mut self.queues);
        let runner = Arc::new(self);
        let started = Instant::now();
        let mut tasks = JoinSet::new();
        for (index, queue) in queues.into_iter().enumerate() {
            let runner = Arc::clone(&runner);
            tasks.spawn(async move { runner.worker(index + 1, queue).await });
        }
        let mut updates = tokio::time::interval(Duration::from_millis(200));
        updates.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let outcome = loop {
            tokio::select! {
                result = tasks.join_next() => match result {
                    Some(result) => match result.context("worker panicked").and_then(|result| result) {
                        Ok(Outcome::Complete) => {},
                        outcome => break outcome,
                    },
                    None => break Ok(Outcome::Complete),
                },
                _ = updates.tick() => {
                    if let Err(error) = runner.progress(started.elapsed(), false).await {
                        break Err(error);
                    }
                },
                signal = tokio::signal::ctrl_c() => {
                    break signal.context("could not listen for Ctrl-C").map(|()| Outcome::Interrupted);
                }
            }
        };
        // All exit paths stop children and leave unfinished cases resumable.
        tasks.shutdown().await;
        runner.store.lock().await.recover()?;
        runner.progress(started.elapsed(), true).await?;
        match outcome? {
            Outcome::Interrupted => {
                println!("Interrupted. Use resume to continue.");
                Ok(ExitCode::from(130))
            }
            Outcome::Complete => {
                let store = runner.store.lock().await;
                store.summary();
                Ok(if store.has_failures() {
                    ExitCode::FAILURE
                } else {
                    ExitCode::SUCCESS
                })
            }
        }
    }

    async fn worker(&self, number: usize, queue: Vec<usize>) -> Result<Outcome> {
        let target = self.targets.join(format!("worker-{number}"));
        for index in queue {
            let (command, args, path) = self.store.lock().await.start(index)?;
            let mut log = File::create(&path)?;
            writeln!(log, "{command}\n")?;
            let status = Command::new("cargo")
                .args(args)
                .current_dir(&self.root)
                .env("CARGO_TARGET_DIR", &target)
                .env("CARGO_BUILD_JOBS", self.build_jobs.to_string())
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .kill_on_drop(true)
                .status()
                .await
                .with_context(|| format!("could not run {command}"))?;
            if interrupted(status) {
                return Ok(Outcome::Interrupted);
            }
            let warnings = read_warnings(&path)?;
            let reason = if !status.success() {
                Some(status.to_string())
            } else if !warnings.is_empty() {
                Some("compiler warnings".into())
            } else {
                None
            };
            self.store.lock().await.finish(index, reason, warnings)?;
        }
        Ok(Outcome::Complete)
    }

    async fn progress(&self, elapsed: Duration, finished: bool) -> Result<()> {
        let terminal = std::io::stdout().is_terminal();
        if !terminal && !finished {
            return Ok(());
        }
        let counts = self.store.lock().await.counts();
        let done = counts.completed();
        let session_done = done - self.initial_completed;
        let rate = session_done as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
        let eta = if session_done == self.session_total {
            "00:00:00".to_owned()
        } else if session_done == 0 {
            "--:--:--".to_owned()
        } else {
            let seconds = ((self.session_total - session_done) as f64 / rate).ceil() as u64;
            format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        };
        let filled = (done * 16).checked_div(self.total).unwrap_or(16);
        let mut stdout = std::io::stdout().lock();
        if terminal {
            write!(stdout, "\r\x1b[2K")?;
        }
        write!(
            stdout,
            "[{}{}] {done}/{} {rate:.1}/s ETA {eta} | {} active | {} FAIL",
            "#".repeat(filled),
            "-".repeat(16 - filled),
            self.total,
            counts.running,
            counts.failed,
        )?;
        if finished {
            writeln!(stdout)?;
        }
        stdout.flush()?;
        Ok(())
    }
}

fn read_warnings(path: &Path) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        let trimmed = line.trim_start();
        if trimmed.starts_with("warning:") || trimmed.starts_with("warning[") {
            warnings.push(line);
        }
    }
    Ok(warnings)
}

fn interrupted(status: ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal() == Some(2)
    }
    #[cfg(not(unix))]
    {
        // Windows STATUS_CONTROL_C_EXIT.
        status.code() == Some(0xC000013Au32 as i32)
    }
}
