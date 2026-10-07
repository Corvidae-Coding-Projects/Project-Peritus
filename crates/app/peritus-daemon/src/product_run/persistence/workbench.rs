//! Fenced-generation recovery; opening state never retries an admitted or staged execution.

use super::{DaemonError, PersistedRecord, RunRecord, quarantine_record, record_path_matches};
use crate::product_control::ControlStore;
use peritus_types::RunId;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(in crate::product_run) fn load_workbench_records(
    root: &Path,
    controls: Option<&ControlStore>,
    finding_bodies: &super::FindingBodyStore,
) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let directory = root.join("runs");
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    let Some(controls) = controls else {
        for path in projection_paths(&directory) {
            quarantine_record(&path, "workbench projection has no governing control store", None);
        }
        return Ok(BTreeMap::new());
    };
    let mut records = BTreeMap::new();
    let quarantine = directory.join(".quarantine");
    let mut paths = projection_paths(&directory);
    paths.extend(
        projection_paths(&quarantine)
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| !directory.join(name).exists())),
    );
    for path in paths {
        let recovering = path.parent() == Some(quarantine.as_path());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                quarantine_record(
                    &path,
                    "workbench projection metadata is unreadable",
                    Some(&error),
                );
                continue;
            }
        };
        if !metadata.file_type().is_file() {
            quarantine_record(&path, "workbench run projection is not a regular file", None);
            continue;
        }
        let (persisted, source_digest) = match read_record(&path) {
            Ok(persisted) => persisted,
            Err(error) => {
                quarantine_record(
                    &path,
                    "workbench execution projection is unreadable or malformed",
                    Some(&error),
                );
                continue;
            }
        };
        let operation = persisted.interaction.workbench.clone();
        if recovering
            && !matches!(
                operation.intent(),
                peritus_product_runner::control::ControlIntent::StartGoal { .. }
            )
        {
            continue;
        }
        let accepted = match controls.resolve(&operation) {
            Ok(resolved) => resolved.is_some(),
            Err(error) => {
                quarantine_record(
                    &path,
                    "workbench start receipt cannot be verified",
                    Some(&error),
                );
                continue;
            }
        };
        let captured = if accepted {
            match controls.capture_execution(&operation) {
                Ok(captured) => Some(captured),
                Err(error) => {
                    quarantine_record(
                        &path,
                        "workbench execution binding cannot be verified",
                        Some(&error),
                    );
                    continue;
                }
            }
        } else {
            if recovering {
                continue;
            }
            None
        };
        let governed = captured.as_ref().map(|capture| capture.inputs().conversation());
        let mut record = match persisted.into_record_with_storage(
            governed,
            Some(root),
            Some(source_digest),
        ) {
            Ok(record) => record,
            Err(error) => {
                quarantine_record(
                    &path,
                    "workbench execution projection contains invalid values",
                    Some(&error),
                );
                continue;
            }
        };
        if let Err(error) = reestablish_record_directory_durability(&directory) {
            quarantine_record(
                &path,
                "workbench execution projection durability cannot be reestablished",
                Some(&error),
            );
            continue;
        }
        if !resume_marker_valid(controls, &operation, &record) {
            quarantine_record(
                &path,
                "goal resume launch marker has no exact control receipt",
                None,
            );
            continue;
        }
        if !continuation_markers_valid(controls, &operation, &record) {
            quarantine_record(
                &path,
                "continuation launch marker has no exact control receipt",
                None,
            );
            continue;
        }
        if !record_path_matches(&path, record.request.run_id()) {
            quarantine_record(
                &path,
                "workbench run filename does not match its embedded identity",
                None,
            );
            continue;
        }
        let mut canonical_changed = false;
        if let Some(governed) = governed {
            match super::replay_handoffs(root, governed, &mut record) {
                Ok(changed) => canonical_changed |= changed,
                Err(error) => {
                    crate::diagnostic::report(&format!(
                        "peritusd: retained incompatible immutable handoff evidence for run {}: {}",
                        record.request.run_id(),
                        error.describe(),
                    ));
                    let changed = !record.handoff_recovery_pending
                        || record.snapshot.phase()
                            != peritus_app_protocol::ProductRunPhase::RecoveryRequired
                        || record.snapshot.status()
                            != "Durable run handoff requires compatible recovery"
                        || record.snapshot.summary() != error.describe()
                        || record.candidate_actionable;
                    record.handoff_recovery_pending = true;
                    record.snapshot = match super::super::replace_snapshot(
                        &record.snapshot,
                        peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                        "Durable run handoff requires compatible recovery",
                        &error.describe(),
                    ) {
                        Ok(snapshot) => snapshot,
                        Err(_) => continue,
                    };
                    record.candidate_actionable = false;
                    canonical_changed |= changed;
                }
            }
        }
        if !record.handoff_recovery_pending
            && record.review_artifact_migration_version
                < super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
        {
            let migration = (|| {
                let mut migrated = record.clone();
                finding_bodies.migrate_record(&mut migrated)?;
                migrated.review_artifact_migration_version =
                    super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION;
                migrated.handoff_sequence = migrated.handoff_sequence.checked_add(1).ok_or_else(
                    || {
                        super::ProductRunServiceError::internal(
                            "publish migrated product finding bodies",
                            "handoff sequence overflow",
                        )
                    },
                )?;
                migrated.handoff_recovery_pending = true;
                super::write_handoff(
                    &root.parent().ok_or_else(|| {
                        super::ProductRunServiceError::internal(
                            "publish migrated product finding bodies",
                            "the workbench root has no state parent",
                        )
                    })?.join("product-runs"),
                    &migrated,
                    super::HandoffKind::Migration,
                )?;
                migrated.handoff_recovery_pending = false;
                record = migrated;
                canonical_changed = true;
                Ok::<(), super::ProductRunServiceError>(())
            })();
            if let Err(error) = migration {
                crate::diagnostic::report(&format!(
                    "peritusd: retained legacy finding authority for run {} after migration stopped: {}",
                    record.request.run_id(),
                    error.describe(),
                ));
                let changed = record.handoff_recovery_pending
                    || record.snapshot.phase()
                        != peritus_app_protocol::ProductRunPhase::RecoveryRequired
                    || record.snapshot.status()
                        != "Product finding bodies require durable migration before this run can continue"
                    || record.snapshot.summary() != error.describe()
                    || record.candidate_actionable;
                record.handoff_recovery_pending = false;
                record.snapshot = match super::super::replace_snapshot(
                    &record.snapshot,
                    peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                    "Product finding bodies require durable migration before this run can continue",
                    &error.describe(),
                ) {
                    Ok(snapshot) => snapshot,
                        Err(_) => continue,
                    };
                record.candidate_actionable = false;
                canonical_changed |= changed;
            }
        }
        if !accepted {
            let changed = record.snapshot.phase()
                != peritus_app_protocol::ProductRunPhase::RecoveryRequired
                || record.snapshot.status()
                    != "Start intent was not committed; retry the exact original operation"
                || record.snapshot.summary() != "No execution admitted by this staged record.";
            record.snapshot = match super::super::replace_snapshot(
                &record.snapshot,
                peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                "Start intent was not committed; retry the exact original operation",
                "No execution admitted by this staged record.",
            ) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    quarantine_record(
                        &path,
                        "workbench staged recovery projection is invalid",
                        Some(&error),
                    );
                    continue;
                }
            };
            canonical_changed |= changed;
        }
        if captured
            .as_ref()
            .is_some_and(|capture| record.interaction.incorporated > capture.inputs().generation())
        {
            quarantine_record(
                &path,
                "workbench incorporation exceeds its authoritative input generation",
                None,
            );
            continue;
        }
        let run = record.request.run_id();
        if records.contains_key(&run) {
            quarantine_record(&path, "duplicate workbench execution identity", None);
            continue;
        }
        if canonical_changed || recovering {
            let Some(parent) = root.parent() else { continue };
            let input = if recovering {
                b"republish-recovered-product-run".as_slice()
            } else {
                b"reconcile-startup-product-run".as_slice()
            };
            if let Err(error) = super::super::publication::persist_startup_record(
                &parent.join("product-runs"),
                &mut record,
                peritus_codec::sha256(input),
            ) {
                crate::diagnostic::report(&format!(
                    "peritusd: could not publish reconciled goal {}: {error}", path.display()
                ));
                continue;
            }
            if recovering {
                crate::diagnostic::report(&format!(
                    "peritusd: recovered validated goal projection {}; original quarantine copy retained",
                    path.display()
                ));
            }
        }
        records.insert(run, record);
    }
    Ok(records)
}

fn resume_marker_valid(
    controls: &ControlStore,
    start: &peritus_product_runner::control::ControlOperation,
    record: &RunRecord,
) -> bool {
    let Some(marker) = record.attempt_admission else { return true };
    controls.operation(start.conversation(), marker).is_ok_and(|operation| {
        operation.is_some_and(|operation| {
            operation.actor_bytes() == start.actor_bytes()
                && operation.workspace_bytes() == start.workspace_bytes()
                && matches!(
                    operation.intent(),
                    peritus_product_runner::control::ControlIntent::ResumeGoal { goal, .. }
                        if *goal == start.id()
                )
                && controls.resolve(&operation).is_ok_and(|receipt| receipt.is_some())
        })
    })
}

fn continuation_markers_valid(
    controls: &ControlStore,
    start: &peritus_product_runner::control::ControlOperation,
    record: &RunRecord,
) -> bool {
    let peritus_product_runner::control::ControlIntent::StartExecution { run, .. } = start.intent()
    else {
        return record.continuation_admissions.is_empty();
    };
    let admissions_valid = record.continuation_admissions.iter().all(|marker| {
        controls.operation(start.conversation(), *marker).is_ok_and(|operation| {
            operation.is_some_and(|operation| {
                operation.actor_bytes() == start.actor_bytes()
                    && operation.workspace_bytes() == start.workspace_bytes()
                    && matches!(
                        operation.intent(),
                        peritus_product_runner::control::ControlIntent::ContinueExecution {
                            run: continued_run,
                            start_operation,
                            ..
                        } if continued_run == run
                            && *start_operation == start.id()
                            && continued_run == record.request.run_id().as_bytes()
                    )
                    && controls.resolve(&operation).is_ok_and(|receipt| receipt.is_some())
            })
        })
    });
    admissions_valid
        && record.continuation_sources.iter().all(|source| {
            record.continuation_admissions.contains(&source.operation)
                && controls
                    .operation(start.conversation(), source.operation)
                    .is_ok_and(|operation| {
                        operation.is_some_and(|operation| {
                            matches!(
                                operation.intent(),
                                peritus_product_runner::control::ControlIntent::ContinueExecution {
                                    context_generation,
                                    ..
                                } if *context_generation == source.generation
                            ) && controls.resolve(&operation).is_ok_and(|receipt| {
                                receipt.is_some_and(|receipt| {
                                    receipt.accepted_revision() == source.revision
                                })
                            }) && controls
                                .load_revision(start.conversation(), source.revision)
                                .is_ok_and(|record| {
                                    record.is_some_and(|record| {
                                        record.inputs().generation() == source.generation
                                            && record.inputs().capture().is_ok_and(|capture| {
                                                !capture.pending().is_empty()
                                            })
                                    })
                                })
                        })
                    })
        })
}

fn projection_paths(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "peritusd: skipped a workbench projection directory entry in {}: {error}",
                    directory.display()
                ));
                None
            }
        })
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| modified(right).cmp(&modified(left)).then_with(|| left.cmp(right)));
    paths
}

fn modified(path: &Path) -> SystemTime {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn read_record(path: &Path) -> std::io::Result<(PersistedRecord, peritus_types::Sha256Digest)> {
    use std::io::{Read as _, Seek as _};
    use sha2::{Digest as _, Sha256};
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "workbench run record is not a regular file",
        ));
    }
    // The atomic published JSON record has no work-size quota. Decode through a fixed physical
    // buffer without retaining a second complete encoded copy or truncating a large record.
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let source_digest = peritus_types::Sha256Digest::new(hasher.finalize().into());
    file.rewind()?;
    let reader = std::io::BufReader::with_capacity(32 * 1024, &file);
    let persisted = serde_json::from_reader(reader)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    file.sync_all()?;
    Ok((persisted, source_digest))
}

fn reestablish_record_directory_durability(
    records: &Path,
) -> Result<(), super::ProductRunServiceError> {
    super::continuation::sync_directory(records, "sync the workbench run directory")?;
    let workbench = records.parent().ok_or_else(|| {
        super::ProductRunServiceError::internal(
            "sync the workbench run directory",
            "the run directory has no workbench parent",
        )
    })?;
    super::continuation::sync_directory(workbench, "sync the workbench root")?;
    if let Some(parent) = workbench.parent().filter(|path| !path.as_os_str().is_empty()) {
        super::continuation::sync_directory(parent, "sync the workbench parent")?;
    }
    Ok(())
}
