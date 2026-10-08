//! Explicit accept, commit, export, and discard operations for completed deliverables.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use peritus_app_protocol::{ProductDeliverable, ProductRunControlAction, ProductRunSnapshot};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::CandidateStage;
use peritus_types::{RunId, Sha256Digest};

use super::snapshot::replace_snapshot;
use super::{
    MutationDisposition, ProductRunService, ProductRunServiceError, RunMutationKind, RunRecord,
};

pub(in crate::product_run) mod commit;
pub(super) mod discard;
#[cfg(test)]
use commit::commit_deliverable;

impl ProductRunService {
    pub(super) fn control_deliverable(
        &self,
        run_id: RunId,
        action: ProductRunControlAction,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let authorities = self.retained_effect_authorities(run_id)?;
        self.with_run_authorities(authorities, || {
            self.control_deliverable_owned(run_id, action)
        })
    }

    fn control_deliverable_owned(
        &self,
        run_id: RunId,
        action: ProductRunControlAction,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let (mut records_view, mut prepared) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone();
            (records.clone(), record)
        };
        if !prepared.snapshot.phase().terminal() {
            return Err(ProductRunServiceError::InvalidState);
        }
        let saved_export = action == ProductRunControlAction::Export
            && prepared.snapshot.deliverable().is_some_and(export_available);
        // Returning an existing immutable export does not depend on mutable-workspace recovery.
        if !saved_export && discard::recover_completed(&self.inner.directory, &mut prepared)? {
            let attempt = Arc::clone(&prepared.cancelled);
            let revision = prepared.record_revision;
            self.publish_deliverable_projection(
                run_id,
                &attempt,
                revision,
                RunMutationKind::Recovery,
                deliverable_input(action, b"recover-completed-discard"),
                prepared,
            )?;
            (records_view, prepared) = {
                let records = self
                    .inner
                    .records
                    .read()
                    .map_err(|_| ProductRunServiceError::Unavailable)?;
                let record =
                    records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone();
                (records.clone(), record)
            };
        }
        let mut deliverable =
            prepared.snapshot.deliverable().cloned().ok_or(ProductRunServiceError::InvalidState)?;
        let pending = discard::Pending::read(&self.inner.directory, &prepared)?;
        if pending.is_some() && action != ProductRunControlAction::Discard && !saved_export {
            return Err(ProductRunServiceError::InvalidState);
        }
        if repeated_action(&prepared, action, &deliverable).is_some() {
            // An earlier attempt may have completed its effects but failed to save
            // the result. A retry must make that result durable before acknowledging it.
            let attempt = Arc::clone(&prepared.cancelled);
            let revision = prepared.record_revision;
            let snapshot = prepared.snapshot.clone();
            let (_, ticket) = self.mutate_run(
                run_id,
                Some(&attempt),
                RunMutationKind::DeliverableEffect,
                deliverable_input(action, b"repeat"),
                MutationDisposition::DurabilityRequired,
                move |record| {
                    if record.record_revision != revision
                        || repeated_action(record, action, &deliverable).is_none()
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                    Ok(())
                },
            )?;
            self.await_run_durable(ticket)?;
            return Ok(snapshot);
        }
        let workspace_id = prepared.request.workspace_id();
        if pending.is_none() {
            discard::workspace_available(&self.inner.directory, &records_view, workspace_id)?;
        }
        let workspace = self
            .inner
            .workspaces
            .get(&prepared.request.workspace_id())
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        if pending.is_some() && Path::new(deliverable.workspace_path()) != workspace.as_path() {
            return Err(ProductRunServiceError::InvalidState);
        }
        if action == ProductRunControlAction::Commit {
            deliverable =
                commit::validate_retry(&self.inner.directory, &mut prepared, &deliverable, &workspace)?;
        } else if pending.is_none() {
            validate_exact_candidate(&prepared, &deliverable, &workspace)?;
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
                let (staged, staging_status) =
                    commit::prepare_recovery(&self.inner.directory, &prepared, deliverable)?;
                prepared.snapshot = replace_snapshot(
                    &prepared.snapshot,
                    prepared.snapshot.phase(),
                    &staging_status,
                    prepared.snapshot.summary(),
                )?
                .with_deliverable(staged.clone());
                let attempt = Arc::clone(&prepared.cancelled);
                let revision = prepared.record_revision;
                self.publish_deliverable_projection(
                    run_id,
                    &attempt,
                    revision,
                    RunMutationKind::DeliverableReservation,
                    deliverable_input(action, b"commit-reservation"),
                    prepared,
                )?;
                prepared = {
                    let records = self
                        .inner
                        .records
                        .read()
                        .map_err(|_| ProductRunServiceError::Unavailable)?;
                    records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone()
                };
                let staged = prepared
                    .snapshot
                    .deliverable()
                    .cloned()
                    .ok_or(ProductRunServiceError::InvalidState)?;
                match commit::commit_prepared(&self.inner.directory, &mut prepared, staged) {
                    Ok(committed) => committed,
                    Err(error) => {
                        let detail = error.to_string();
                        let detail = &detail[..detail.floor_char_boundary(detail.len().min(8192))];
                        let display = prepared
                            .snapshot
                            .deliverable()
                            .map(ProductDeliverable::export_path)
                            .unwrap_or_default();
                        prepared.snapshot = replace_snapshot(
                            &prepared.snapshot,
                            prepared.snapshot.phase(),
                            &format!(
                                "Commit did not complete: {detail}. Source patch saved to {display}"
                            ),
                            prepared.snapshot.summary(),
                        )?;
                        let attempt = Arc::clone(&prepared.cancelled);
                        let revision = prepared.record_revision;
                        self.publish_deliverable_projection(
                            run_id,
                            &attempt,
                            revision,
                            RunMutationKind::DeliverableEffect,
                            deliverable_input(action, b"commit-failed"),
                            prepared,
                        )?;
                        return Err(error);
                    }
                }
            }
            ProductRunControlAction::Export => {
                let path = export_deliverable(
                    &self.inner.directory,
                    run_id,
                    &deliverable,
                    prepared.task_baseline.as_deref(),
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
                drop(discard::Reservation::prepare(
                    &self.inner.directory,
                    &prepared,
                    &deliverable,
                )?);
                if pending.is_none() {
                    let _ = discard::Pending::prepare(
                        &self.inner.directory,
                        &prepared,
                        &workspace,
                    )?;
                }
                let attempt = Arc::clone(&prepared.cancelled);
                let revision = prepared.record_revision;
                self.publish_deliverable_projection(
                    run_id,
                    &attempt,
                    revision,
                    RunMutationKind::DeliverableReservation,
                    deliverable_input(action, b"discard-reservation"),
                    prepared,
                )?;
                prepared = {
                    let records = self
                        .inner
                        .records
                        .read()
                        .map_err(|_| ProductRunServiceError::Unavailable)?;
                    records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone()
                };
                let completion = discard::Reservation::prepare(
                    &self.inner.directory,
                    &prepared,
                    &deliverable,
                )?;
                let pending = discard::Pending::read(&self.inner.directory, &prepared)?;
                let recovered = if let Some(pending) = pending {
                    match pending.execute(&self.inner.directory, &prepared) {
                        Ok(paths) => paths,
                        Err(error) => {
                            discard::Pending::mark_interrupted(&mut prepared)?;
                            let attempt = Arc::clone(&prepared.cancelled);
                            let revision = prepared.record_revision;
                            self.publish_deliverable_projection(
                                run_id,
                                &attempt,
                                revision,
                                RunMutationKind::DeliverableEffect,
                                deliverable_input(action, b"discard-interrupted"),
                                prepared,
                            )?;
                            return Err(error);
                        }
                    }
                } else {
                    let baseline = prepared
                        .task_baseline
                        .as_deref()
                        .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
                    ProductRunner::discard_from_baseline(
                        &workspace,
                        baseline,
                        deliverable.changed_paths(),
                    )
                    .map_err(|error| {
                        ProductRunServiceError::internal(
                            "restore task preimages",
                            error.to_string(),
                        )
                    })?
                };
                let status = discard_status(&recovered);
                completion.complete(&status)?;
                (deliverable.mark_discarded(), status)
            }
            ProductRunControlAction::Cancel
            | ProductRunControlAction::Retry
            | ProductRunControlAction::Acknowledge => {
                return Err(ProductRunServiceError::InvalidState);
            }
        };
        let snapshot = replace_snapshot(
            &prepared.snapshot,
            prepared.snapshot.phase(),
            &status,
            prepared.snapshot.summary(),
        )?
        .with_deliverable(deliverable);
        prepared.snapshot = snapshot;
        if action == ProductRunControlAction::Discard {
            discard::Pending::clear_interruption(&mut prepared);
        }
        let attempt = Arc::clone(&prepared.cancelled);
        let revision = prepared.record_revision;
        self.publish_deliverable_projection(
            run_id,
            &attempt,
            revision,
            RunMutationKind::DeliverableEffect,
            deliverable_input(action, b"complete"),
            prepared,
        )
    }

    fn publish_deliverable_projection(
        &self,
        run: RunId,
        attempt: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        expected_revision: u64,
        kind: RunMutationKind,
        input: Sha256Digest,
        prepared: RunRecord,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.inner.product_artifacts.publish_record(&prepared)?;
        let (snapshot, ticket) = self.mutate_run(
            run,
            Some(attempt),
            kind,
            input,
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.record_revision < expected_revision {
                    return Err(ProductRunServiceError::InvalidState);
                }
                apply_prepared_projection(record, prepared, expected_revision)?;
                Ok(record.snapshot.clone())
            },
        )?;
        self.await_run_durable(ticket)?;
        Ok(snapshot)
    }
}

fn apply_prepared_projection(
    record: &mut RunRecord,
    prepared: RunRecord,
    expected_revision: u64,
) -> Result<(), ProductRunServiceError> {
    if record.attempt_sequence != prepared.attempt_sequence
        || record.handoff_sequence != prepared.handoff_sequence
        || record.request.run_id() != prepared.request.run_id()
        || record.request.workspace_id() != prepared.request.workspace_id()
        || record.interaction.workbench != prepared.interaction.workbench
    {
        return Err(ProductRunServiceError::InvalidState);
    }
    let retained_boundary = (record.record_revision > expected_revision
        && matches!(
            record.snapshot.phase(),
            peritus_app_protocol::ProductRunPhase::Cancelled
                | peritus_app_protocol::ProductRunPhase::RecoveryRequired
        ))
        .then(|| {
            (
                record.snapshot.phase(),
                record.snapshot.status().to_owned(),
                record.snapshot.summary().to_owned(),
            )
        });
    record.snapshot = prepared.snapshot;
    record.checkpoint = prepared.checkpoint;
    record.settlement = prepared.settlement;
    record.resume = prepared.resume;
    record.candidate_actionable = prepared.candidate_actionable;
    record.interruption_cause = prepared.interruption_cause;
    record.remaining_work = prepared.remaining_work;
    if let Some((phase, status, summary)) = retained_boundary {
        record.snapshot = replace_snapshot(&record.snapshot, phase, &status, &summary)?;
    }
    Ok(())
}

fn deliverable_input(action: ProductRunControlAction, stage: &[u8]) -> Sha256Digest {
    let action = match action {
        ProductRunControlAction::Accept => b"accept".as_slice(),
        ProductRunControlAction::Commit => b"commit".as_slice(),
        ProductRunControlAction::Export => b"export".as_slice(),
        ProductRunControlAction::Discard => b"discard".as_slice(),
        ProductRunControlAction::Cancel => b"cancel".as_slice(),
        ProductRunControlAction::Retry => b"retry".as_slice(),
        ProductRunControlAction::Acknowledge => b"acknowledge".as_slice(),
    };
    let mut input = Vec::with_capacity(action.len() + stage.len() + 1);
    input.extend_from_slice(action);
    input.push(0);
    input.extend_from_slice(stage);
    peritus_codec::sha256(&input)
}

fn discard_status(recovered: &[PathBuf]) -> String {
    let mut status = "Deliverable discarded".to_owned();
    if !recovered.is_empty() {
        status.push_str("\nGit history was preserved. Restored nested repositories may use a detached HEAD or a new unborn branch; recovery records below explain how to revisit saved work.");
    }
    for path in recovered {
        status.push_str(&format!("\nRepository recovery: {}", path.display()));
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
        ProductRunControlAction::Cancel
        | ProductRunControlAction::Retry
        | ProductRunControlAction::Acknowledge => false,
    };
    already_done.then(|| record.snapshot.clone())
}

pub(in crate::product_run) fn export_available(deliverable: &ProductDeliverable) -> bool {
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
    let checkpoint = record.checkpoint.as_ref().ok_or(ProductRunServiceError::InvalidState)?;
    if checkpoint.stage() != deliverable.qualification()
        || checkpoint.identity().run_id() != record.request.run_id()
        || checkpoint.identity().workspace_id() != record.request.workspace_id()
    {
        return Err(ProductRunServiceError::InvalidState);
    }
    let current = ProductRunner::candidate_digest(Path::new(deliverable.workspace_path()))
        .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?;
    if current != checkpoint.identity().repository_digest() {
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
        match baseline {
            Some(baseline) => ProductRunner::candidate_patch_from_baseline(root, baseline)
                .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?,
            #[cfg(test)]
            None => uncommitted_patch(root, deliverable.changed_paths())?,
            #[cfg(not(test))]
            None => return Err(ProductRunServiceError::WorkspaceUnavailable),
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn tracked(root: &Path, path: &str) -> Result<bool, ProductRunServiceError> {
    Command::new("git")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .map_err(|_| ProductRunServiceError::Unavailable)
}

#[cfg(all(test, windows))]
const fn null_device() -> &'static str {
    "NUL"
}

#[cfg(all(test, not(windows)))]
const fn null_device() -> &'static str {
    "/dev/null"
}

pub(super) fn run_hex(run_id: RunId) -> String {
    run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    })
}

#[cfg(test)]
#[path = "deliverable/tests.rs"]
mod tests;
