//! Explicit accept, commit, export, and discard operations for completed deliverables.

use std::{
    fs,
    io::Write as _,
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

mod commit;
pub(super) mod discard;
#[cfg(test)]
use commit::commit_deliverable;

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
        let saved_export = action == ProductRunControlAction::Export
            && record.snapshot.deliverable().is_some_and(export_available);
        // Returning an existing immutable export does not depend on mutable-workspace recovery.
        if !saved_export && discard::recover_completed(&self.inner.directory, record)? {
            persist_record(&self.inner.directory, record)?;
        }
        let mut deliverable =
            record.snapshot.deliverable().cloned().ok_or(ProductRunServiceError::InvalidState)?;
        let pending = discard::Pending::read(&self.inner.directory, record)?;
        if pending.is_some() && action != ProductRunControlAction::Discard && !saved_export {
            return Err(ProductRunServiceError::InvalidState);
        }
        if let Some(snapshot) = repeated_action(record, action, &deliverable) {
            // An earlier attempt may have completed its effects but failed to save
            // the result. A retry must make that result durable before acknowledging it.
            persist_record(&self.inner.directory, record)?;
            return Ok(snapshot);
        }
        let workspace_id = record.request.workspace_id();
        if pending.is_none() {
            discard::workspace_available(&self.inner.directory, &records, workspace_id)?;
        }
        let record = records.get_mut(&run_id).ok_or(ProductRunServiceError::NotFound)?;
        let workspace = self
            .inner
            .workspaces
            .get(&record.request.workspace_id())
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        if pending.is_some() && Path::new(deliverable.workspace_path()) != workspace {
            return Err(ProductRunServiceError::InvalidState);
        }
        if action == ProductRunControlAction::Commit {
            deliverable =
                commit::validate_retry(&self.inner.directory, record, &deliverable, workspace)?;
        } else if pending.is_none() {
            validate_exact_candidate(record, &deliverable, workspace)?;
        }
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
                commit::with_recovery(&self.inner.directory, record, deliverable)?
            }
            ProductRunControlAction::Export => {
                if record.task_baseline_required && record.task_baseline.is_none() {
                    return Err(ProductRunServiceError::WorkspaceUnavailable);
                }
                let path = export_deliverable(
                    &self.inner.directory,
                    run_id,
                    &deliverable,
                    record.task_baseline.as_deref(),
                )?;
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
                let completion =
                    discard::Reservation::prepare(&self.inner.directory, record, &deliverable)?;
                let trace = self.inner.directory.join(format!("{}.trace", run_hex(run_id)));
                let pending = match pending {
                    Some(pending) => Some(pending),
                    None => discard::Pending::prepare(&self.inner.directory, record, workspace)?,
                };
                let recovered = if let Some(pending) = pending {
                    match pending.execute(&self.inner.directory, record) {
                        Ok(paths) => paths,
                        Err(error) => {
                            discard::Pending::mark_interrupted(record)?;
                            persist_record(&self.inner.directory, record)?;
                            return Err(error);
                        }
                    }
                } else if let Some(baseline) = &record.task_baseline {
                    ProductRunner::discard_from_baseline(
                        workspace,
                        baseline,
                        deliverable.changed_paths(),
                    )
                    .map_err(|error| {
                        ProductRunServiceError::internal(
                            "restore task preimages",
                            error.to_string(),
                        )
                    })?
                } else if record.task_baseline_required {
                    return Err(ProductRunServiceError::WorkspaceUnavailable);
                } else if let Some(recovered) = ProductRunner::discard_task_candidate(
                    workspace,
                    &trace,
                    deliverable.changed_paths(),
                )
                .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?
                {
                    recovered
                } else {
                    discard_deliverable(&deliverable)?;
                    Vec::new()
                };
                let status = discard_status(&recovered);
                completion.complete(&status)?;
                (deliverable.mark_discarded(), status)
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

fn discard_status(recovered: &[PathBuf]) -> String {
    let mut status = "Deliverable discarded".to_owned();
    if !recovered.is_empty() {
        status.push_str("\nGit history was preserved. Restored nested repositories may use a detached HEAD or a new unborn branch; recovery records below explain how to revisit saved work.");
    }
    for (index, path) in recovered.iter().enumerate() {
        let line = format!("\nRepository recovery: {}", path.display());
        if status.len().saturating_add(line.len())
            > peritus_app_protocol::MAX_PRODUCT_DETAIL_BYTES - 256
        {
            use std::fmt::Write as _;
            let _ = write!(
                status,
                "\n{} additional recovery records are in peritus/discarded under their enclosing Git directories.",
                recovered.len() - index
            );
            break;
        }
        status.push_str(&line);
    }
    status
}

fn repeated_action(
    record: &super::RunRecord,
    action: ProductRunControlAction,
    deliverable: &ProductDeliverable,
) -> Option<ProductRunSnapshot> {
    let already_done = match action {
        ProductRunControlAction::Accept => deliverable.accepted(),
        ProductRunControlAction::Commit => !deliverable.commit_revision().is_empty(),
        ProductRunControlAction::Export => export_available(deliverable),
        ProductRunControlAction::Discard => deliverable.discarded(),
        ProductRunControlAction::Cancel | ProductRunControlAction::Retry => false,
    };
    already_done.then(|| record.snapshot.clone())
}

fn export_available(deliverable: &ProductDeliverable) -> bool {
    !deliverable.export_path().is_empty() && Path::new(deliverable.export_path()).is_file()
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

fn export_deliverable(
    product_run_directory: &Path,
    run_id: RunId,
    deliverable: &ProductDeliverable,
    baseline: Option<&str>,
) -> Result<PathBuf, ProductRunServiceError> {
    let root = Path::new(deliverable.workspace_path());
    let bytes = if deliverable.commit_revision().is_empty() {
        let trace = product_run_directory.join(format!("{}.trace", run_hex(run_id)));
        match baseline
            .map(|baseline| ProductRunner::candidate_patch_from_baseline(root, baseline))
            .transpose()
            .and_then(|embedded| match embedded {
                Some(bytes) => Ok(Some(bytes)),
                None => ProductRunner::task_candidate_patch(root, &trace),
            })
            .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?
        {
            Some(patch) => patch,
            None => uncommitted_patch(root, deliverable.changed_paths())?,
        }
    } else {
        let output = Command::new("git")
            .env("GIT_LITERAL_PATHSPECS", "1")
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
    let mut file = fs::File::create(&temporary).map_err(|_| ProductRunServiceError::Unavailable)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    drop(file);
    fs::rename(temporary, &path).map_err(|_| ProductRunServiceError::Unavailable)?;
    #[cfg(unix)]
    fs::File::open(&directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    Ok(path)
}

fn uncommitted_patch(root: &Path, paths: &[String]) -> Result<Vec<u8>, ProductRunServiceError> {
    let output = Command::new("git")
        .env("GIT_LITERAL_PATHSPECS", "1")
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
            .env("GIT_LITERAL_PATHSPECS", "1")
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
            .env("GIT_LITERAL_PATHSPECS", "1")
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
        .env("GIT_LITERAL_PATHSPECS", "1")
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
