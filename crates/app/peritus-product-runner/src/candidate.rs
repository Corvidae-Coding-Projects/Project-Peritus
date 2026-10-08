//! Exact current candidate paths relative to the managed worktree HEAD.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

pub mod managed;
pub(crate) mod evidence;
pub(crate) mod process;

use crate::workspace_filter;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Validated managed-worktree candidate reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateBaseline {
    head: String,
    managed: Option<managed::ManagedBaseline>,
    in_place: Option<crate::workspace_delivery::scope::ScopedBaseline>,
}

/// One backend-owned observation of every axis used to publish a candidate identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CandidateObservation {
    has_workspace_candidate: bool,
    content: peritus_types::Sha256Digest,
    repository: peritus_types::Sha256Digest,
}

impl CandidateObservation {
    pub(crate) const fn axes(self) -> (peritus_types::Sha256Digest, peritus_types::Sha256Digest) {
        (self.content, self.repository)
    }

    pub(crate) const fn into_parts(
        self,
    ) -> (bool, peritus_types::Sha256Digest, peritus_types::Sha256Digest) {
        (self.has_workspace_candidate, self.content, self.repository)
    }
}

impl CandidateBaseline {
    /// Validates that the managed workspace has a committed comparison base.
    pub fn capture(root: &Path) -> Result<Self, ProductRunnerError> {
        let mut command = Command::new("git");
        command.args(["rev-parse", "--verify", "HEAD"]).current_dir(root);
        let output = git_output(command, "resolve candidate base")?;
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
            return baseline.changed_paths(root);
        }
        let mut paths = BTreeSet::new();
        let mut command = Command::new("git");
        command
            .args(["diff", "--no-ext-diff", "--name-only", "-z"])
            .arg(&self.head)
            .arg("--")
            .current_dir(root);
        let tracked = git_output(command, "list tracked candidate paths")?;
        if !tracked.status.success() {
            return Err(repository(
                "list tracked candidate paths",
                "git could not compare the managed worktree with the captured run baseline",
            ));
        }
        append_paths(&tracked.stdout, &mut paths, CandidatePathKind::Tracked)?;
        let mut command = Command::new("git");
        command.args(["ls-files", "--others", "--exclude-standard", "-z"]).current_dir(root);
        let untracked = git_output(command, "list untracked candidate paths")?;
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
        if let Some(scope) = &self.in_place {
            return crate::progress::WorkspaceCheckpoint::scoped(root, scope.paths()?);
        }
        if let Some(managed) = &self.managed {
            return managed.candidate_checkpoint(root);
        }
        crate::progress::WorkspaceCheckpoint::capture(root)
    }

    pub(crate) fn content_digest(
        &self,
        root: &Path,
    ) -> Result<peritus_types::Sha256Digest, ProductRunnerError> {
        // An in-place run has no repository-history axis. Its exact enrolled file snapshot is both
        // the source-content observation and the handoff fence.
        if let Some(scope) = &self.in_place {
            return scope.progress_checkpoint(root).map(|checkpoint| checkpoint.digest());
        }
        if let Some(managed) = &self.managed {
            return managed.candidate_content_digest(root);
        }
        managed::ManagedBaseline::source_digest(root)
    }

    /// Captures presence, content, and repository identity from one snapshot-capable backend read.
    /// Legacy Git baselines return `None` so their caller can fence independent observations.
    pub(crate) fn snapshot_observation(
        &self,
        root: &Path,
    ) -> Result<Option<CandidateObservation>, ProductRunnerError> {
        if let Some(managed) = &self.managed {
            let snapshot = managed.evidence_snapshot(root)?;
            return Ok(Some(CandidateObservation {
                has_workspace_candidate: !snapshot.entries.is_empty(),
                content: snapshot.content_digest,
                repository: snapshot.repository_digest,
            }));
        }
        if let Some(scope) = &self.in_place {
            let snapshot = scope.evidence_snapshot(root)?;
            return Ok(Some(CandidateObservation {
                has_workspace_candidate: !snapshot.entries.is_empty(),
                content: snapshot.content_digest,
                repository: snapshot.repository_digest,
            }));
        }
        Ok(None)
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
        let mut command = Command::new("git");
        command.args(["rev-parse", "--verify", "HEAD"]).current_dir(&nested);
        let head = git_output(command, "inspect nested candidate repository")?;
        if !head.status.success() {
            continue;
        }
        let mut command = Command::new("git");
        command.args(["diff", "--name-only", "-z", "HEAD", "--"]).current_dir(&nested);
        let changed = git_output(command, "list nested candidate changes")?;
        if !changed.status.success() {
            return Err(repository(
                "list nested candidate changes",
                "git could not compare the nested repository with its HEAD",
            ));
        }
        append_prefixed_paths(&relative, &changed.stdout, paths, CandidatePathKind::Tracked)?;
        let mut command = Command::new("git");
        command.args(["ls-files", "--others", "--exclude-standard", "-z"]).current_dir(&nested);
        let untracked = git_output(command, "list nested untracked paths")?;
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

struct GitOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
}

fn git_output(
    command: Command,
    operation: &'static str,
) -> Result<GitOutput, ProductRunnerError> {
    let mut stdout = Vec::new();
    let completed = process::stream_current(command, None, &mut stdout, operation)?;
    Ok(GitOutput { status: completed.status, stdout })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn candidate_survives_restart_and_includes_new_modified_and_deleted_files() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("modified.txt"), "before").expect("write");
        fs::write(root.path().join("deleted.txt"), "before").expect("write");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        fs::write(root.path().join("modified.txt"), "after").expect("write");
        fs::remove_file(root.path().join("deleted.txt")).expect("delete");
        fs::write(root.path().join("new.txt"), "new").expect("write");

        let first = CandidateBaseline::capture(root.path()).expect("baseline");
        let second = CandidateBaseline::capture(root.path()).expect("restart baseline");
        let expected = vec![
            PathBuf::from("deleted.txt"),
            PathBuf::from("modified.txt"),
            PathBuf::from("new.txt"),
        ];
        assert_eq!(first.changed_paths(root.path()).expect("changes"), expected);
        assert_eq!(second.changed_paths(root.path()).expect("changes"), expected);
    }

    #[test]
    fn candidate_keeps_tracked_build_paths_but_omits_untracked_build_products() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::create_dir(root.path().join("build")).expect("build directory");
        fs::write(root.path().join("build/maintained.py"), "VALUE = 1\n").expect("source");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        fs::write(root.path().join("build/maintained.py"), "VALUE = 2\n").expect("modify");
        fs::write(root.path().join("build/generated.py"), "VALUE = 3\n").expect("generated");

        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");

        assert_eq!(
            baseline.changed_paths(root.path()).expect("changes"),
            vec![PathBuf::from("build/maintained.py")]
        );
    }

    #[test]
    fn candidate_includes_changes_committed_after_the_run_baseline() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("site.md"), "before\n").expect("baseline source");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");

        fs::write(root.path().join("site.md"), "after\n").expect("updated source");
        run(root.path(), &["add", "site.md"]);
        run(root.path(), &["commit", "--quiet", "-m", "update site"]);

        assert_eq!(
            baseline.changed_paths(root.path()).expect("committed changes"),
            vec![PathBuf::from("site.md")]
        );
    }

    #[test]
    fn candidate_expands_dirty_files_inside_an_untracked_nested_repository() {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        run(root.path(), &["commit", "--quiet", "--allow-empty", "-m", "fixture"]);
        let nested = root.path().join("imported");
        fs::create_dir(&nested).expect("nested root");
        run(&nested, &["init", "--quiet"]);
        run(&nested, &["config", "user.email", "peritus@example.invalid"]);
        run(&nested, &["config", "user.name", "Peritus Test"]);
        fs::write(nested.join("changed.rs"), "pub const VALUE: u8 = 1;\n").expect("source");
        run(&nested, &["add", "."]);
        run(&nested, &["commit", "--quiet", "-m", "nested fixture"]);
        fs::write(nested.join("changed.rs"), "pub const VALUE: u8 = 2;\n").expect("modify");
        fs::write(nested.join("new.rs"), "pub const NEW: u8 = 3;\n").expect("untracked");
        fs::create_dir(nested.join("target")).expect("generated directory");
        fs::write(nested.join("target/ignored.rs"), "generated\n").expect("generated");

        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
        let expected = vec![
            PathBuf::from("imported"),
            PathBuf::from("imported/changed.rs"),
            PathBuf::from("imported/new.rs"),
        ];
        assert_eq!(baseline.changed_paths(root.path()).expect("changes"), expected);
    }

    fn run(root: &Path, arguments: &[&str]) {
        assert!(
            Command::new("git").args(arguments).current_dir(root).status().expect("git").success()
        );
    }
}
