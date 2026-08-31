//! Exact current candidate paths relative to the managed worktree HEAD.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use crate::workspace_filter;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Validated managed-worktree candidate reference.
pub struct CandidateBaseline {
    head: Vec<u8>,
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
        Ok(Self { head: output.stdout })
    }

    /// Returns every tracked modification/deletion and nonignored untracked file against HEAD.
    ///
    /// Using the committed worktree base keeps the same candidate set through daemon restart and
    /// retry instead of silently rebasing around unfinished agent work.
    pub fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let head = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(root)
            .output()
            .map_err(|error| repository("recheck candidate base", error.to_string()))?;
        if !head.status.success() || head.stdout != self.head {
            return Err(repository(
                "recheck candidate base",
                "managed worktree HEAD changed during the coding run",
            ));
        }
        let mut paths = BTreeSet::new();
        let tracked = Command::new("git")
            .args(["diff", "--name-only", "-z", "HEAD", "--"])
            .current_dir(root)
            .output()
            .map_err(|error| repository("list tracked candidate paths", error.to_string()))?;
        if !tracked.status.success() {
            return Err(repository(
                "list tracked candidate paths",
                "git could not compare the managed worktree with HEAD",
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
