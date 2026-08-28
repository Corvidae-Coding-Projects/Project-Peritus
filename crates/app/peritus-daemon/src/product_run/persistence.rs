//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductProviderSelection, ProductRunPhase, ProductRunRequest, ProductRunSnapshot,
};
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use super::{ProductRunServiceError, RunRecord, filesystem, invalid};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

#[derive(Serialize, Deserialize)]
struct PersistedRecord {
    run_id: String,
    workspace_id: String,
    writer: String,
    reviewer: String,
    fixer: String,
    phase: u16,
    cycle: u32,
    task: String,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
}

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let persisted = PersistedRecord::from_snapshot(&record.snapshot);
    let bytes =
        serde_json::to_vec_pretty(&persisted).map_err(|_| ProductRunServiceError::Unavailable)?;
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, bytes).map_err(|_| ProductRunServiceError::Unavailable)?;
    fs::rename(temporary, path).map_err(|_| ProductRunServiceError::Unavailable)
}

pub(super) fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let mut records = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(filesystem)? {
        let entry = entry.map_err(filesystem)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(filesystem)?;
        let persisted: PersistedRecord = serde_json::from_slice(&bytes).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Reconcile,
                "load product run",
                "product-run state is malformed",
                error,
            )
        })?;
        let record = persisted
            .into_record()
            .map_err(|_| invalid("product-run state contains invalid values"))?;
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
}

impl PersistedRecord {
    fn from_snapshot(snapshot: &ProductRunSnapshot) -> Self {
        let providers = snapshot.providers();
        Self {
            run_id: hex(snapshot.run_id().as_bytes()),
            workspace_id: hex(snapshot.workspace_id().as_bytes()),
            writer: hex(providers.writer().as_bytes()),
            reviewer: hex(providers.reviewer().as_bytes()),
            fixer: hex(providers.fixer().as_bytes()),
            phase: snapshot.phase().tag(),
            cycle: snapshot.cycle(),
            task: snapshot.task().to_owned(),
            status: snapshot.status().to_owned(),
            diff: snapshot.diff().to_owned(),
            gates: snapshot.gates().to_owned(),
            review: snapshot.review().to_owned(),
            summary: snapshot.summary().to_owned(),
        }
    }

    fn into_record(self) -> Result<RunRecord, ProductRunServiceError> {
        let run_id =
            RunId::new(unhex(&self.run_id)?).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let workspace_id = WorkspaceId::new(unhex(&self.workspace_id)?)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let providers = ProductProviderSelection::new(
            profile(&self.writer)?,
            profile(&self.reviewer)?,
            profile(&self.fixer)?,
        );
        let request = ProductRunRequest::new(run_id, workspace_id, providers, self.task.clone())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let loaded_phase =
            ProductRunPhase::from_tag(self.phase).ok_or(ProductRunServiceError::InvalidMessage)?;
        let (phase, status) = if loaded_phase.terminal() {
            (loaded_phase, self.status)
        } else {
            (
                ProductRunPhase::RecoveryRequired,
                "Daemon restart interrupted this run; retry is available".to_owned(),
            )
        };
        let snapshot = ProductRunSnapshot::new(
            run_id,
            workspace_id,
            providers,
            phase,
            self.cycle,
            self.task,
            status,
            self.diff,
            self.gates,
            self.review,
            self.summary,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        Ok(RunRecord {
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
        })
    }
}

fn profile(value: &str) -> Result<ProviderProfileId, ProductRunServiceError> {
    ProviderProfileId::new(unhex(value)?).map_err(|_| ProductRunServiceError::InvalidMessage)
}

fn hex(bytes: &[u8; 16]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use core::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn unhex(value: &str) -> Result<[u8; 16], ProductRunServiceError> {
    if value.len() != 32 {
        return Err(ProductRunServiceError::InvalidMessage);
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        bytes[index] =
            u8::from_str_radix(text, 16).map_err(|_| ProductRunServiceError::InvalidMessage)?;
    }
    Ok(bytes)
}
