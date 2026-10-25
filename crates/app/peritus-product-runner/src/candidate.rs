//! Exact current candidate paths relative to the managed worktree HEAD.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

pub use peritus_workspace::task_candidate::managed;

use crate::workspace_filter;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Validated managed-worktree candidate reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateBaseline {
    head: String,
    managed: Option<managed::ManagedBaseline>,
    in_place: Option<crate::workspace_delivery::scope::ScopedBaseline>,
}

impl CandidateBaseline {
    /// Validates that the managed workspace has a committed comparison base.
    pub fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        let output = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(root)
            .output()
            .map_err(|error| repository("resolve candidate base", error.to_string()))?;
        if !output.status.success() {
            return Err(repository(
                "resolve candidate base",
                "managed workspace has no committed HEAD",
            ));
        }
        let head = String::from_utf8(output.stdout)
            .map_err(|_| repository("resolve candidate base", "Git HEAD is not UTF-8"))?;
        let head = head.trim().to_owned();
        if head.is_empty() {
            return Err(repository(
                "resolve candidate base",
                "managed workspace has no committed HEAD",
            ));
        }
        Ok(Self { head, in_place: None, managed: None })
    }

    pub(crate) fn restored(head: String) -> Result<Self, ProductRunnerError> {
        if !matches!(head.len(), 40 | 64) || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(repository(
                "restore candidate base",
                "durable candidate base is not a Git object identifier",
            ));
        }
        Ok(Self { head, in_place: None, managed: None })
    }

    /// Returns every tracked modification/deletion and nonignored untracked file against the
    /// captured run baseline.
    ///
    /// Comparing the current tree with the captured commit retains changes that the coding task
    /// legitimately committed, merged, or carried across a branch switch.
    pub fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        if let Some(scope) = &self.in_place {
            return scope.changed_paths(root);
        }
        if let Some(baseline) = &self.managed {
            return baseline.changed_paths(root).map_err(Into::into);
        }
        let mut paths = BTreeSet::new();
        let tracked = Command::new("git")
            .args(["diff", "--no-ext-diff", "--name-only", "-z"])
            .arg(&self.head)
            .arg("--")
            .current_dir(root)
            .output()
            .map_err(|error| repository("list tracked candidate paths", error.to_string()))?;
        if !tracked.status.success() {
            return Err(repository(
                "list tracked candidate paths",
                "git could not compare the managed worktree with the captured run baseline",
            ));
        }
        append_paths(&tracked.stdout, &mut paths, CandidatePathKind::Tracked)?;
        let untracked = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard", "-z"])
            .current_dir(root)
            .output()
            .map_err(|error| repository("list untracked candidate paths", error.to_string()))?;
        if !untracked.status.success() {
            return Err(repository(
                "list untracked candidate paths",
                "git could not enumerate untracked candidate files",
            ));
        }
        append_paths(&untracked.stdout, &mut paths, CandidatePathKind::Untracked)?;
        append_nested_repository_changes(root, &mut paths)?;
        Ok(paths.into_iter().collect())
    }

    pub(crate) fn capture_task(root: &Path, trace: &Path) -> Result<Self, ProductRunnerError> {
        let mut baseline = Self::capture(root)?;
        let path = trace.with_extension("baseline");
        let managed = if let Some(retained) = managed::ManagedBaseline::load(&path)? {
            retained
        } else {
            let captured = managed::ManagedBaseline::capture(root, true)?;
            captured.save(&path)?;
            captured
        };
        baseline.managed = Some(managed);
        Ok(baseline)
    }

    pub(crate) const fn managed(&self) -> Option<&managed::ManagedBaseline> {
        self.managed.as_ref()
    }

    pub(crate) fn with_managed(
        mut self,
        managed: Option<managed::ManagedBaseline>,
    ) -> Result<Self, ProductRunnerError> {
        if let Some(value) = &managed {
            value.validate()?;
            if self.in_place.is_some() {
                return Err(repository("restore baseline", "mixed workspace kinds"));
            }
        }
        self.managed = managed;
        Ok(self)
    }

    pub(crate) fn head(&self) -> &str {
        &self.head
    }

    pub(crate) const fn in_place(scope: crate::workspace_delivery::scope::ScopedBaseline) -> Self {
        Self { head: String::new(), in_place: Some(scope), managed: None }
    }

    pub(crate) const fn scope(&self) -> Option<&crate::workspace_delivery::scope::ScopedBaseline> {
        self.in_place.as_ref()
    }

    pub(crate) fn checkpoint(
        &self,
        root: &Path,
    ) -> Result<crate::progress::WorkspaceCheckpoint, ProductRunnerError> {
        match &self.in_place {
            Some(scope) => crate::progress::WorkspaceCheckpoint::scoped(root, scope.paths()?),
            None => crate::progress::WorkspaceCheckpoint::capture(root),
        }
    }

    pub(crate) fn content_digest(
        &self,
        root: &Path,
    ) -> Result<peritus_types::Sha256Digest, ProductRunnerError> {
        // An in-place run has no repository-history axis. Its exact enrolled file snapshot is both
        // the source-content observation and the handoff fence.
        self.in_place.as_ref().map_or_else(
            || managed::ManagedBaseline::source_digest(root).map_err(Into::into),
            |scope| scope.progress_checkpoint(root).map(|checkpoint| checkpoint.digest()),
        )
    }
}

fn append_nested_repository_changes(
    root: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<(), ProductRunnerError> {
    let nested_roots = paths
        .iter()
        .filter(|relative| {
            root.join(relative).is_dir() && root.join(relative).join(".git").exists()
        })
        .cloned()
        .collect::<Vec<_>>();
    for relative in nested_roots {
        let nested = root.join(&relative);
        let head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&nested)
            .output()
            .map_err(|error| {
                repository("inspect nested candidate repository", error.to_string())
            })?;
        if !head.status.success() {
            continue;
        }
        let changed = Command::new("git")
            .args(["diff", "--name-only", "-z", "HEAD", "--"])
            .current_dir(&nested)
            .output()
            .map_err(|error| repository("list nested candidate changes", error.to_string()))?;
        if !changed.status.success() {
            return Err(repository(
                "list nested candidate changes",
                "git could not compare the nested repository with its HEAD",
            ));
        }
        append_prefixed_paths(&relative, &changed.stdout, paths, CandidatePathKind::Tracked)?;
        let untracked = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard", "-z"])
            .current_dir(&nested)
            .output()
            .map_err(|error| repository("list nested untracked paths", error.to_string()))?;
        if !untracked.status.success() {
            return Err(repository(
                "list nested untracked paths",
                "git could not enumerate nested untracked files",
            ));
        }
        append_prefixed_paths(&relative, &untracked.stdout, paths, CandidatePathKind::Untracked)?;
    }
    Ok(())
}

fn append_prefixed_paths(
    prefix: &Path,
    encoded: &[u8],
    paths: &mut BTreeSet<PathBuf>,
    kind: CandidatePathKind,
) -> Result<(), ProductRunnerError> {
    for value in encoded.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let relative = std::str::from_utf8(value).map(PathBuf::from).map_err(|_| {
            repository("decode nested candidate path", "workspace path is not UTF-8")
        })?;
        let path = prefix.join(relative);
        if kind.retains(&path) {
            paths.insert(path);
        }
    }
    Ok(())
}

fn append_paths(
    encoded: &[u8],
    paths: &mut BTreeSet<PathBuf>,
    kind: CandidatePathKind,
) -> Result<(), ProductRunnerError> {
    for value in encoded.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let path = std::str::from_utf8(value)
            .map(PathBuf::from)
            .map_err(|_| repository("decode candidate path", "workspace path is not UTF-8"))?;
        if kind.retains(&path) {
            paths.insert(path);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CandidatePathKind {
    Tracked,
    Untracked,
}

impl CandidatePathKind {
    fn retains(self, path: &Path) -> bool {
        matches!(self, Self::Tracked) || !workspace_filter::generated(path)
    }
}

fn repository(operation: &'static str, detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, detail)
}
