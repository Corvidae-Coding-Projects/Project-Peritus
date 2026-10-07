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
        let persisted = match read_record(&path) {
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
        let mut record = match persisted.into_record_with_context(
            captured.as_ref().map(|capture| capture.inputs().conversation()),
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
        if !resume_marker_valid(controls, &operation, &record) {
            quarantine_record(
                &path,
                "goal resume launch marker has no exact control receipt",
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
        if !accepted {
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
        if recovering {
            let Some(parent) = root.parent() else { continue };
            if let Err(error) = super::write_record(&parent.join("product-runs"), &record) {
                crate::diagnostic::report(&format!(
                    "peritusd: could not republish validated goal {}: {error}",
                    path.display()
                ));
                continue;
            }
            crate::diagnostic::report(&format!(
                "peritusd: recovered validated goal projection {}; original quarantine copy retained",
                path.display()
            ));
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
    let Some(marker) = record.goal_resume else { return true };
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

fn read_record(path: &Path) -> std::io::Result<PersistedRecord> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "workbench run record is not a regular file",
        ));
    }
    // Decode unchanged JSON through one physical buffer without a work-size quota or a
    // second complete encoded copy. Opening recovery preserves its existing read-only access.
    let reader = std::io::BufReader::with_capacity(32 * 1024, file);
    serde_json::from_reader(reader)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn modified(path: &Path) -> SystemTime {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}
