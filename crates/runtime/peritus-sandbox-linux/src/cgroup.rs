//! Delegated cgroup-v2 resource ownership and cleanup.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, ResourcePlan};
use peritus_types::Sha256Digest;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REQUIRED_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Runtime cgroup-v2 delegation facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CgroupSupport {
    root: PathBuf,
    unified: bool,
    available_controllers: BTreeSet<String>,
    delegated_controllers: BTreeSet<String>,
    writable: bool,
}

impl CgroupSupport {
    /// Probes one explicit cgroup parent without mutating it.
    #[must_use]
    pub fn probe(root: &Path) -> Self {
        let available_controllers = read_words(&root.join("cgroup.controllers"));
        let delegated_controllers = read_words(&root.join("cgroup.subtree_control"));
        let unified = root.join("cgroup.type").exists() || root.join("cgroup.controllers").exists();
        let writable = can_open_write(&root.join("cgroup.procs"))
            && can_open_write(&root.join("cgroup.subtree_control"));
        Self {
            root: root.to_path_buf(),
            unified,
            available_controllers,
            delegated_controllers,
            writable,
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn unavailable(root: PathBuf) -> Self {
        Self {
            root,
            unified: false,
            available_controllers: BTreeSet::new(),
            delegated_controllers: BTreeSet::new(),
            writable: false,
        }
    }

    /// Returns the probed parent.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Reports a unified cgroup hierarchy.
    #[must_use]
    pub const fn unified(&self) -> bool {
        self.unified
    }
    /// Reports exact controller delegation and write access.
    #[must_use]
    pub fn delegated(&self) -> bool {
        self.unified
            && self.writable
            && REQUIRED_CONTROLLERS.iter().all(|name| {
                self.available_controllers.contains(*name)
                    && self.delegated_controllers.contains(*name)
            })
    }
}

/// Deterministic exact cgroup leaf and controller values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CgroupPlan {
    root: PathBuf,
    leaf: PathBuf,
    memory_max: u64,
    pids_max: u64,
    cpu_max: String,
}

impl CgroupPlan {
    /// Creates one exact digest-named leaf below a proved delegated parent.
    ///
    /// # Errors
    /// Returns unsupported if the parent does not delegate all required controllers.
    pub fn new(
        support: &CgroupSupport,
        preparation_digest: Sha256Digest,
        resources: ResourcePlan,
    ) -> Result<Self, LinuxError> {
        if !support.delegated() {
            return Err(cgroup_error(
                LinuxOperation::Prepare,
                LinuxRecovery::ConfigureHost,
                "cgroup v2 parent lacks writable cpu, memory, and pids delegation",
            ));
        }
        let suffix = &crate::canonical::digest_hex(preparation_digest)[..24];
        let leaf = support.root.join(format!("peritus-{suffix}"));
        if leaf.parent() != Some(support.root.as_path()) {
            return Err(cgroup_error(
                LinuxOperation::Prepare,
                LinuxRecovery::CorrectRequest,
                "cgroup leaf escaped its configured parent",
            ));
        }
        Ok(Self {
            root: support.root.clone(),
            leaf,
            memory_max: resources.memory_bytes(),
            pids_max: resources.processes(),
            cpu_max: "100000 100000".to_owned(),
        })
    }
    /// Returns the exact leaf path.
    #[must_use]
    pub fn leaf(&self) -> &Path {
        &self.leaf
    }
    /// Creates and configures the leaf. Existing leaves are rejected rather than adopted.
    ///
    /// # Errors
    /// Returns a cgroup failure on any partial installation; successfully created partial state is
    /// removed before returning where possible.
    pub fn install(&self) -> Result<CgroupHandle, LinuxError> {
        fs::create_dir(&self.leaf).map_err(|error| {
            LinuxError::io(LinuxOperation::Install, "create exact cgroup leaf", &error)
        })?;
        let result = (|| {
            write_value(&self.leaf.join("memory.max"), &self.memory_max.to_string())?;
            write_value(&self.leaf.join("pids.max"), &self.pids_max.to_string())?;
            write_value(&self.leaf.join("cpu.max"), &self.cpu_max)?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_dir(&self.leaf);
            return Err(error);
        }
        Ok(CgroupHandle { root: self.root.clone(), leaf: Some(self.leaf.clone()) })
    }
}

/// Result of exact idempotent native cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupOutcome {
    /// Exact leaf was removed and no owned process remains.
    Complete,
    /// Leaf was already absent.
    AlreadyClean,
}

/// Owned installed cgroup leaf.
#[derive(Debug)]
pub struct CgroupHandle {
    root: PathBuf,
    leaf: Option<PathBuf>,
}

impl CgroupHandle {
    pub(crate) const fn reopen_exact(root: PathBuf, leaf: PathBuf) -> Self {
        Self { root, leaf: Some(leaf) }
    }
    /// Returns the exact current leaf.
    #[must_use]
    pub fn leaf(&self) -> Option<&Path> {
        self.leaf.as_deref()
    }
    /// Kills, drains, and removes only the exact owned leaf.
    ///
    /// # Errors
    /// Returns indeterminate when the path no longer has the exact parent or cannot be observed.
    pub fn cleanup(&mut self) -> Result<CleanupOutcome, LinuxError> {
        let Some(leaf) = self.leaf.clone() else {
            return Ok(CleanupOutcome::AlreadyClean);
        };
        if leaf.parent() != Some(self.root.as_path()) {
            return Err(cgroup_error(
                LinuxOperation::Release,
                LinuxRecovery::Quarantine,
                "cgroup identity escaped its exact parent",
            ));
        }
        if !leaf.exists() {
            self.leaf = None;
            return Ok(CleanupOutcome::AlreadyClean);
        }
        if leaf.join("cgroup.kill").exists() {
            write_value(&leaf.join("cgroup.kill"), "1")?;
        }
        let started = Instant::now();
        while populated(&leaf)? {
            if started.elapsed() >= CLEANUP_TIMEOUT {
                return Err(cgroup_error(
                    LinuxOperation::Release,
                    LinuxRecovery::RetryCleanup,
                    "cgroup remained populated past cleanup bound",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        fs::remove_dir(&leaf).map_err(|error| {
            LinuxError::io(LinuxOperation::Release, "remove exact cgroup leaf", &error)
        })?;
        self.leaf = None;
        Ok(CleanupOutcome::Complete)
    }
}

impl Drop for CgroupHandle {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn read_words(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .split_whitespace()
        .map(|value| value.trim_start_matches('+').to_owned())
        .collect()
}

fn can_open_write(path: &Path) -> bool {
    OpenOptions::new().write(true).open(path).is_ok()
}

fn write_value(path: &Path, value: &str) -> Result<(), LinuxError> {
    let mut file = OpenOptions::new().write(true).open(path).map_err(|error| {
        LinuxError::io(LinuxOperation::Install, "open cgroup controller", &error)
    })?;
    file.write_all(value.as_bytes())
        .map_err(|error| LinuxError::io(LinuxOperation::Install, "write cgroup controller", &error))
}

fn populated(leaf: &Path) -> Result<bool, LinuxError> {
    let events = fs::read_to_string(leaf.join("cgroup.events"))
        .map_err(|error| LinuxError::io(LinuxOperation::Release, "read cgroup events", &error))?;
    Ok(events.lines().any(|line| line.split_whitespace().eq(["populated", "1"])))
}

fn cgroup_error(
    operation: LinuxOperation,
    recovery: LinuxRecovery,
    detail: &'static str,
) -> LinuxError {
    LinuxError::new(LinuxErrorKind::Cgroup, operation, recovery, detail)
}
