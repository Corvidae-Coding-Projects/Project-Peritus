//! Explicit accept, commit, export, and discard operations for completed deliverables.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use peritus_app_protocol::{
    ProductDeliverable, ProductRunControlAction, ProductRunPhase, ProductRunSnapshot,
};
use peritus_types::RunId;

use super::persistence::persist_record;
use super::snapshot::replace_snapshot;
use super::{ProductRunService, ProductRunServiceError};

impl ProductRunService {
    pub(super) fn control_deliverable(
        &self,
        run_id: RunId,
        action: ProductRunControlAction,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run_id).ok_or(ProductRunServiceError::NotFound)?;
        if record.snapshot.phase() != ProductRunPhase::Complete {
            return Err(ProductRunServiceError::InvalidState);
        }
        let deliverable =
            record.snapshot.deliverable().cloned().ok_or(ProductRunServiceError::InvalidState)?;
        if deliverable.discarded() {
            return Err(ProductRunServiceError::InvalidState);
        }
        let (deliverable, status) = match action {
            ProductRunControlAction::Accept => {
                (deliverable.mark_accepted(), "Deliverable accepted".to_owned())
            }
            ProductRunControlAction::Commit => {
                if !deliverable.commit_revision().is_empty() {
                    return Err(ProductRunServiceError::InvalidState);
                }
                let revision = commit_deliverable(&deliverable, record.request.task())?;
                (
                    deliverable
                        .mark_committed(revision.clone())
                        .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                    format!("Deliverable committed as {revision}"),
                )
            }
            ProductRunControlAction::Export => {
                let path = export_deliverable(&self.inner.directory, run_id, &deliverable)?;
                let display = path.to_string_lossy().into_owned();
                (
                    deliverable
                        .mark_exported(display.clone())
                        .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                    format!("Deliverable exported to {display}"),
                )
            }
            ProductRunControlAction::Discard => {
                if !deliverable.commit_revision().is_empty() {
                    return Err(ProductRunServiceError::InvalidState);
                }
                discard_deliverable(&deliverable)?;
                (deliverable.mark_discarded(), "Deliverable discarded".to_owned())
            }
            ProductRunControlAction::Cancel | ProductRunControlAction::Retry => {
                return Err(ProductRunServiceError::InvalidState);
            }
        };
        let snapshot = replace_snapshot(
            &record.snapshot,
            ProductRunPhase::Complete,
            &status,
            record.snapshot.summary(),
        )?
        .with_deliverable(deliverable);
        record.snapshot = snapshot;
        persist_record(&self.inner.directory, record)?;
        Ok(record.snapshot.clone())
    }
}

fn commit_deliverable(
    deliverable: &ProductDeliverable,
    task: &str,
) -> Result<String, ProductRunServiceError> {
    let root = Path::new(deliverable.workspace_path());
    let add = Command::new("git")
        .arg("add")
        .arg("--")
        .args(deliverable.changed_paths())
        .current_dir(root)
        .status()
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    if !add.success() {
        return Err(ProductRunServiceError::Unavailable);
    }
    let subject = task.lines().next().unwrap_or("completed task").trim();
    let subject = &subject[..subject.floor_char_boundary(subject.len().min(64))];
    let commit = Command::new("git")
        .args(["commit", "--only", "-m", &format!("peritus: {subject}"), "--"])
        .args(deliverable.changed_paths())
        .current_dir(root)
        .status()
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    if !commit.success() {
        return Err(ProductRunServiceError::Unavailable);
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    if !revision.status.success() {
        return Err(ProductRunServiceError::Unavailable);
    }
    String::from_utf8(revision.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(|_| ProductRunServiceError::Unavailable)
}

fn export_deliverable(
    product_run_directory: &Path,
    run_id: RunId,
    deliverable: &ProductDeliverable,
) -> Result<PathBuf, ProductRunServiceError> {
    let root = Path::new(deliverable.workspace_path());
    let bytes = if deliverable.commit_revision().is_empty() {
        uncommitted_patch(root, deliverable.changed_paths())?
    } else {
        let output = Command::new("git")
            .args(["format-patch", "-1", "--stdout", deliverable.commit_revision()])
            .current_dir(root)
            .output()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        if !output.status.success() {
            return Err(ProductRunServiceError::Unavailable);
        }
        output.stdout
    };
    if bytes.is_empty() {
        return Err(ProductRunServiceError::InvalidState);
    }
    let directory = product_run_directory.parent().unwrap_or(product_run_directory).join("exports");
    fs::create_dir_all(&directory).map_err(|_| ProductRunServiceError::Unavailable)?;
    let path = directory.join(format!("{}.patch", run_hex(run_id)));
    let temporary = path.with_extension("patch.new");
    fs::write(&temporary, bytes).map_err(|_| ProductRunServiceError::Unavailable)?;
    #[cfg(windows)]
    if path.is_file() {
        fs::remove_file(&path).map_err(|_| ProductRunServiceError::Unavailable)?;
    }
    fs::rename(temporary, &path).map_err(|_| ProductRunServiceError::Unavailable)?;
    Ok(path)
}

fn uncommitted_patch(root: &Path, paths: &[String]) -> Result<Vec<u8>, ProductRunServiceError> {
    let output = Command::new("git")
        .arg("diff")
        .arg("--binary")
        .arg("HEAD")
        .arg("--")
        .args(paths)
        .current_dir(root)
        .output()
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    if !output.status.success() {
        return Err(ProductRunServiceError::Unavailable);
    }
    let mut patch = output.stdout;
    for path in paths {
        if tracked(root, path)? || !root.join(path).is_file() {
            continue;
        }
        let output = Command::new("git")
            .args(["diff", "--no-index", "--binary", "--", null_device(), path])
            .current_dir(root)
            .output()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(ProductRunServiceError::Unavailable);
        }
        patch.extend_from_slice(&output.stdout);
    }
    Ok(patch)
}

fn discard_deliverable(deliverable: &ProductDeliverable) -> Result<(), ProductRunServiceError> {
    let root = Path::new(deliverable.workspace_path());
    let mut tracked_paths = Vec::new();
    for path in deliverable.changed_paths() {
        if tracked(root, path)? {
            tracked_paths.push(path.as_str());
        } else {
            match fs::remove_file(root.join(path)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(ProductRunServiceError::Unavailable),
            }
        }
    }
    if !tracked_paths.is_empty() {
        let status = Command::new("git")
            .args(["restore", "--staged", "--worktree", "--source=HEAD", "--"])
            .args(tracked_paths)
            .current_dir(root)
            .status()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        if !status.success() {
            return Err(ProductRunServiceError::Unavailable);
        }
    }
    Ok(())
}

fn tracked(root: &Path, path: &str) -> Result<bool, ProductRunServiceError> {
    Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .map_err(|_| ProductRunServiceError::Unavailable)
}

#[cfg(windows)]
const fn null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
const fn null_device() -> &'static str {
    "/dev/null"
}

fn run_hex(run_id: RunId) -> String {
    run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;
    use tempfile::TempDir;

    #[test]
    fn export_and_discard_are_limited_to_exact_deliverable_paths() {
        let repository = repository();
        fs::write(repository.path().join("chosen.txt"), "changed\n").expect("chosen change");
        fs::write(repository.path().join("unrelated.txt"), "unrelated change\n")
            .expect("unrelated change");
        fs::write(repository.path().join("new.txt"), "new file\n").expect("new file");
        let deliverable = ProductDeliverable::new(
            repository.path().to_string_lossy().into_owned(),
            vec!["chosen.txt".to_owned(), "new.txt".to_owned()],
            vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
        )
        .expect("deliverable");

        let patch = String::from_utf8(
            uncommitted_patch(repository.path(), deliverable.changed_paths()).expect("patch"),
        )
        .expect("UTF-8 patch");
        assert!(patch.contains("chosen.txt"));
        assert!(patch.contains("new.txt"));
        assert!(!patch.contains("unrelated.txt"));

        discard_deliverable(&deliverable).expect("discard");
        assert_eq!(
            fs::read_to_string(repository.path().join("chosen.txt")).expect("chosen"),
            "base\n"
        );
        assert!(!repository.path().join("new.txt").exists());
        assert_eq!(
            fs::read_to_string(repository.path().join("unrelated.txt")).expect("unrelated"),
            "unrelated change\n"
        );
    }

    #[test]
    fn commit_excludes_an_unrelated_pre_staged_change() {
        let repository = repository();
        fs::write(repository.path().join("chosen.txt"), "changed\n").expect("chosen change");
        fs::write(repository.path().join("unrelated.txt"), "unrelated change\n")
            .expect("unrelated change");
        git(repository.path(), &["add", "--", "unrelated.txt"]);
        let deliverable = ProductDeliverable::new(
            repository.path().to_string_lossy().into_owned(),
            vec!["chosen.txt".to_owned()],
            vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
        )
        .expect("deliverable");

        let revision = commit_deliverable(&deliverable, "finish the game").expect("commit");
        assert_eq!(revision.len(), 40);
        let committed =
            git_output(repository.path(), &["show", "--format=", "--name-only", "HEAD"]);
        assert!(committed.contains("chosen.txt"));
        assert!(!committed.contains("unrelated.txt"));
        let staged = git_output(repository.path(), &["diff", "--cached", "--name-only"]);
        assert_eq!(staged.trim(), "unrelated.txt");
    }

    fn repository() -> TempDir {
        let repository = TempDir::new().expect("temporary repository");
        git(repository.path(), &["init", "--quiet"]);
        git(repository.path(), &["config", "user.name", "Peritus Test"]);
        git(repository.path(), &["config", "user.email", "peritus@example.invalid"]);
        git(repository.path(), &["config", "commit.gpgsign", "false"]);
        fs::write(repository.path().join("chosen.txt"), "base\n").expect("chosen base");
        fs::write(repository.path().join("unrelated.txt"), "base\n").expect("unrelated base");
        git(repository.path(), &["add", "--", "chosen.txt", "unrelated.txt"]);
        git(repository.path(), &["commit", "--quiet", "-m", "initial"]);
        repository
    }

    fn git(root: &Path, arguments: &[&str]) {
        let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
        assert_success(output);
    }

    fn git_output(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git").args(arguments).current_dir(root).output().expect("git");
        assert_success(output.clone());
        String::from_utf8(output.stdout).expect("UTF-8 git output")
    }

    fn assert_success(output: Output) {
        assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
    }
}
