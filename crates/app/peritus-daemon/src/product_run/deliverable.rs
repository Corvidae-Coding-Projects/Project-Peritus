//! Explicit accept, commit, export, and discard operations for completed deliverables.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use peritus_app_protocol::{ProductDeliverable, ProductRunControlAction, ProductRunSnapshot};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::CandidateStage;
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
        if !record.snapshot.phase().terminal() {
            return Err(ProductRunServiceError::InvalidState);
        }
        let deliverable =
            record.snapshot.deliverable().cloned().ok_or(ProductRunServiceError::InvalidState)?;
        if let Some(snapshot) = repeated_action(record, action, &deliverable) {
            return Ok(snapshot);
        }
        let workspace = self
            .inner
            .workspaces
            .get(&record.request.workspace_id())
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        validate_exact_candidate(record, &deliverable, workspace)?;
        let (deliverable, status) = match action {
            ProductRunControlAction::Accept => {
                let status = if deliverable.qualification() == CandidateStage::Qualified {
                    "Qualified deliverable accepted"
                } else {
                    "Unqualified candidate accepted by explicit user choice"
                };
                (deliverable.mark_accepted(), status.to_owned())
            }
            ProductRunControlAction::Commit => {
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
            record.snapshot.phase(),
            &status,
            record.snapshot.summary(),
        )?
        .with_deliverable(deliverable);
        record.snapshot = snapshot;
        persist_record(&self.inner.directory, record)?;
        Ok(record.snapshot.clone())
    }
}

fn repeated_action(
    record: &super::RunRecord,
    action: ProductRunControlAction,
    deliverable: &ProductDeliverable,
) -> Option<ProductRunSnapshot> {
    let already_done = match action {
        ProductRunControlAction::Accept => deliverable.accepted(),
        ProductRunControlAction::Commit => !deliverable.commit_revision().is_empty(),
        ProductRunControlAction::Export => !deliverable.export_path().is_empty(),
        ProductRunControlAction::Discard => deliverable.discarded(),
        ProductRunControlAction::Cancel | ProductRunControlAction::Retry => false,
    };
    already_done.then(|| record.snapshot.clone())
}

fn validate_exact_candidate(
    record: &super::RunRecord,
    deliverable: &ProductDeliverable,
    workspace: &Path,
) -> Result<(), ProductRunServiceError> {
    if deliverable.discarded() {
        return Err(ProductRunServiceError::InvalidState);
    }
    if !record.candidate_actionable {
        return Err(ProductRunServiceError::InvalidState);
    }
    if !deliverable.commit_revision().is_empty() {
        return Ok(());
    }
    if Path::new(deliverable.workspace_path()) != workspace {
        return Err(ProductRunServiceError::InvalidState);
    }
    let Some(checkpoint) = record.checkpoint.as_ref() else {
        // Legacy qualified handoffs predate digest persistence and remain operable.
        return (deliverable.qualification() == CandidateStage::Qualified)
            .then_some(())
            .ok_or(ProductRunServiceError::InvalidState);
    };
    if checkpoint.stage() != deliverable.qualification()
        || checkpoint.identity().run_id() != record.request.run_id()
        || checkpoint.identity().workspace_id() != record.request.workspace_id()
    {
        return Err(ProductRunServiceError::InvalidState);
    }
    let current = ProductRunner::candidate_digest(Path::new(deliverable.workspace_path()))
        .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?;
    if current != checkpoint.identity().candidate_digest() {
        return Err(ProductRunServiceError::InvalidState);
    }
    Ok(())
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
#[path = "deliverable/tests.rs"]
mod tests;
