use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Passed,
    Failed,
}

#[derive(Serialize, Deserialize)]
pub struct Case {
    pub command: String,
    pub args: Vec<String>,
    pub status: Status,
    pub attempts: usize,
    pub log: Option<PathBuf>,
    pub reason: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: u32,
    pub root: PathBuf,
    pub enumeration: Vec<String>,
    pub cases: Vec<Case>,
}

#[derive(Default)]
pub struct Counts {
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
    pub running: usize,
}

impl Counts {
    pub fn completed(&self) -> usize {
        self.passed + self.failed
    }
}

pub struct Store {
    pub path: PathBuf,
    data: Option<Checkpoint>,
    _lock: File,
    events: File,
    failed: BTreeSet<usize>,
}

impl Store {
    pub fn clean(self) -> Result<()> {
        let Self {
            path,
            events,
            _lock: lock,
            ..
        } = self;
        drop(events);
        let mut files = vec![path.clone()];
        for extension in ["events.jsonl", "failed.txt", "json.tmp", "failed.tmp"] {
            files.push(path.with_extension(extension));
        }
        for file in files {
            match fs::remove_file(&file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("could not remove {}", file.display()));
                }
            }
        }
        let directory = path.with_extension("logs");
        match fs::remove_dir_all(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("could not remove {}", directory.display()));
            }
        }
        // Keep the lock file in place so another process cannot lock a new inode
        // while cleanup still holds the old one.
        drop(lock);
        println!("Cleaned checkpoint and logs: {}", path.display());
        Ok(())
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        fs::create_dir_all(path.parent().context("state path has no parent")?)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))?;
        lock.try_lock()
            .context("checkpoint is already in use by another harness")?;
        let events = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path.with_extension("events.jsonl"))?;
        Ok(Self {
            path,
            data: None,
            _lock: lock,
            events,
            failed: BTreeSet::new(),
        })
    }

    pub fn create(&mut self, data: Checkpoint, overwrite: bool) -> Result<()> {
        ensure!(
            overwrite || !self.path.exists(),
            "checkpoint already exists; use resume, retry-failed, or run --overwrite"
        );
        self.failed.clear();
        self.events.set_len(0)?;
        self.data = Some(data);
        self.save()
    }

    pub fn load(&mut self, root: &Path) -> Result<()> {
        let mut data: Checkpoint = serde_json::from_slice(
            &fs::read(&self.path).context("no checkpoint; start with run")?,
        )?;
        ensure!(data.version == 1, "unsupported checkpoint version");
        ensure!(
            data.root == root,
            "checkpoint belongs to a different workspace"
        );
        let bytes = fs::read(self.path.with_extension("events.jsonl"))?;
        let valid = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        self.events.set_len(valid as u64)?;
        for line in bytes[..valid]
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let (index, case): (usize, Case) = serde_json::from_slice(line)?;
            let existing = data
                .cases
                .get_mut(index)
                .context("invalid checkpoint event")?;
            ensure!(
                existing.command == case.command,
                "checkpoint command mismatch"
            );
            *existing = case;
        }
        self.failed = data
            .cases
            .iter()
            .enumerate()
            .filter(|(_, case)| case.status == Status::Failed)
            .map(|(index, _)| index)
            .collect();
        self.data = Some(data);
        self.recover()?;
        self.export_failures()
    }

    pub fn root(&self) -> &Path {
        &self.data.as_ref().expect("checkpoint loaded").root
    }

    pub fn cases(&self) -> &[Case] {
        &self.data.as_ref().expect("checkpoint loaded").cases
    }

    fn case_mut(&mut self, index: usize) -> &mut Case {
        &mut self.data.as_mut().expect("checkpoint loaded").cases[index]
    }

    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for case in self.cases() {
            match case.status {
                Status::Pending => counts.pending += 1,
                Status::Running => counts.running += 1,
                Status::Passed => counts.passed += 1,
                Status::Failed => counts.failed += 1,
            }
        }
        counts
    }

    pub fn pending(&mut self, index: usize) -> Result<()> {
        let case = self.case_mut(index);
        case.status = Status::Pending;
        case.reason = None;
        case.warnings.clear();
        self.record(index)
    }

    pub fn start(&mut self, index: usize) -> Result<(String, Vec<String>, PathBuf)> {
        let logs = self.path.with_extension("logs");
        let case = self.case_mut(index);
        case.status = Status::Running;
        case.attempts += 1;
        let log = logs.join(format!("{:05}-attempt-{}.log", index + 1, case.attempts));
        case.log = Some(log.clone());
        case.reason = None;
        case.warnings.clear();
        let work = (case.command.clone(), case.args.clone(), log);
        self.record(index)?;
        Ok(work)
    }

    pub fn finish(
        &mut self,
        index: usize,
        reason: Option<String>,
        warnings: Vec<String>,
    ) -> Result<()> {
        let case = self.case_mut(index);
        case.status = if reason.is_some() {
            Status::Failed
        } else {
            Status::Passed
        };
        case.reason = reason;
        case.warnings = warnings;
        self.record(index)
    }

    pub fn recover(&mut self) -> Result<()> {
        for index in 0..self.cases().len() {
            if self.cases()[index].status == Status::Running {
                self.pending(index)?;
            }
        }
        Ok(())
    }

    fn record(&mut self, index: usize) -> Result<()> {
        let case = &self.cases()[index];
        let failed = case.status == Status::Failed;
        let mut line = serde_json::to_vec(&(index, case))?;
        line.push(b'\n');
        self.events.write_all(&line)?;
        self.events.sync_all()?;
        let changed = if failed {
            self.failed.insert(index)
        } else {
            self.failed.remove(&index)
        };
        if changed {
            self.export_failures()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let data = self.data.as_ref().context("checkpoint not initialized")?;
        let temp = self.path.with_extension("json.tmp");
        let mut file = File::create(&temp)?;
        serde_json::to_writer_pretty(&mut file, data)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        fs::rename(temp, &self.path)?;
        self.export_failures()
    }

    fn export_failures(&self) -> Result<()> {
        // Snapshot plus journal are authoritative; this is a convenience export.
        let commands: String = self
            .failed
            .iter()
            .map(|index| format!("{}\n", self.cases()[*index].command))
            .collect();
        let temp = self.path.with_extension("failed.tmp");
        fs::write(&temp, commands)?;
        fs::rename(temp, self.path.with_extension("failed.txt"))?;
        Ok(())
    }

    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    pub fn summary(&self) {
        let counts = self.counts();
        println!(
            "Saved: {} passed, {} failed, {} unfinished.\nState: {}\nFailed commands: {}",
            counts.passed,
            counts.failed,
            counts.pending + counts.running,
            self.path.display(),
            self.path.with_extension("failed.txt").display()
        );
        for (index, case) in self
            .cases()
            .iter()
            .enumerate()
            .filter(|(_, case)| case.status == Status::Failed)
        {
            println!(
                "FAIL #{}: {}\n  {}\n  Log: {}",
                index + 1,
                case.reason.as_deref().unwrap_or("unknown"),
                case.command,
                case.log
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
            );
        }
    }
}
