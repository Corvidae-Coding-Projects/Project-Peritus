//! Fenced-generation recovery; opening state never retries an admitted or staged execution.

use super::{DaemonError, PersistedRecord, RunRecord, filesystem, invalid};
use crate::product_control::ControlStore;
use peritus_types::RunId;
use std::{collections::BTreeMap, path::Path};

const MAX_RUN_RECORD_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RUN_RECORDS: usize = 1024;

pub(in crate::product_run) fn load_workbench_records(
    root: &Path,
    controls: Option<&ControlStore>,
) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let directory = root.join("runs");
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    let controls = controls
        .ok_or_else(|| invalid("workbench run generation has no governing control store"))?;
    let mut records = BTreeMap::new();
    for entry in std::fs::read_dir(directory).map_err(filesystem)? {
        let entry = entry.map_err(filesystem)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry.metadata().map_err(filesystem)?;
        if !metadata.is_file()
            || metadata.len() > MAX_RUN_RECORD_BYTES
            || records.len() >= MAX_RUN_RECORDS
        {
            return Err(invalid("workbench run projection exceeds its generation bounds"));
        }
        let bytes = std::fs::read(entry.path()).map_err(filesystem)?;
        let persisted: PersistedRecord = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("workbench execution projection is malformed"))?;
        let operation = persisted
            .interaction
            .as_ref()
            .and_then(|options| options.workbench.as_ref())
            .ok_or_else(|| invalid("workbench run projection has no exact start binding"))?;
        let accepted = controls
            .resolve(operation)
            .map_err(|_| invalid("workbench start receipt cannot be verified"))?
            .is_some();
        let captured = if accepted {
            Some(
                controls
                    .capture_execution(operation)
                    .map_err(|_| invalid("workbench execution binding cannot be verified"))?,
            )
        } else {
            None
        };
        let mut record = persisted
            .into_record_with_context(
                captured.as_ref().map(|capture| capture.inputs().conversation()),
            )
            .map_err(|_| invalid("workbench execution projection contains invalid values"))?;
        if !accepted {
            record.snapshot = super::super::replace_snapshot(
                &record.snapshot,
                peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                "Start intent was not committed; retry the exact original operation",
                "No execution admitted by this staged record.",
            )
            .map_err(|_| invalid("workbench staged recovery projection is invalid"))?;
        }
        if captured.as_ref().is_some_and(|capture| {
            record
                .interaction
                .as_ref()
                .is_some_and(|options| options.incorporated > capture.inputs().generation())
        }) {
            return Err(invalid(
                "workbench incorporation exceeds its authoritative input generation",
            ));
        }
        let run = record.request.run_id();
        if records.insert(run, record).is_some() {
            return Err(invalid("duplicate workbench execution identity"));
        }
    }
    Ok(records)
}
