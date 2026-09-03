//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationMessage, ProductConversationRole, ProductDeliverable,
    ProductProviderSelection, ProductRunPhase, ProductRunRequest, ProductRunSnapshot,
};
use peritus_product_runner::{ConversationView, ProductRunResume};
use peritus_provider_core::CancellationToken;
use peritus_run_settlement::CandidateStage;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use serde::Deserialize;
use serde::Serialize;

use super::progress::RunProgress;
use super::{ProductRunServiceError, RunRecord, SharedConversation, filesystem, invalid};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

mod progress;
mod settlement;

use settlement::{PersistedCheckpoint, restore_settlement};

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
    #[serde(default)]
    finding_state: String,
    #[serde(default)]
    deliverable: Option<PersistedDeliverable>,
    #[serde(default)]
    messages: Vec<PersistedMessage>,
    #[serde(default)]
    progress: PersistedProgress,
    #[serde(default)]
    checkpoint: Option<PersistedCheckpoint>,
    #[serde(default)]
    settlement_cause: Option<u16>,
    #[serde(default)]
    resume_state: Option<Vec<u8>>,
    #[serde(default)]
    remaining_work: Vec<String>,
    #[serde(default)]
    interruption_cause: String,
    #[serde(default)]
    candidate_actionable: Option<bool>,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedProgress {
    started_unix_millis: u64,
    last_effect_unix_millis: u64,
    model_requests: u32,
    tool_calls: u32,
    retries: u32,
    #[serde(default)]
    provider_failovers: u32,
    compactions: u32,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    provider_cost_microunits: u64,
    usage_observations: u32,
    #[serde(default)]
    workspace_bytes: u64,
    #[serde(default)]
    workspace_growth_bytes: u64,
    #[serde(default)]
    peak_rss_bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct PersistedMessage {
    role: u16,
    content: String,
}

#[derive(Serialize, Deserialize)]
struct PersistedDeliverable {
    workspace_path: String,
    changed_paths: Vec<String>,
    successful_commands: Vec<String>,
    run_instructions: String,
    #[serde(default)]
    qualification: Option<u16>,
    accepted: bool,
    commit_revision: String,
    export_path: String,
    discarded: bool,
}

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let persisted = PersistedRecord::from_record(record)?;
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
    fn from_record(record: &RunRecord) -> Result<Self, ProductRunServiceError> {
        let snapshot = &record.snapshot;
        let providers = snapshot.providers();
        let messages = record
            .conversation
            .messages()?
            .into_iter()
            .map(|message| PersistedMessage {
                role: message.role().tag(),
                content: message.content().to_owned(),
            })
            .collect();
        Ok(Self {
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
            finding_state: record.finding_state.clone(),
            deliverable: snapshot.deliverable().map(PersistedDeliverable::from_deliverable),
            messages,
            progress: PersistedProgress::from_run(&record.progress),
            checkpoint: record.checkpoint.as_ref().map(PersistedCheckpoint::from_checkpoint),
            settlement_cause: record.settlement.as_ref().map(|value| value.cause().tag()),
            resume_state: record
                .resume
                .as_ref()
                .map(ProductRunResume::encode_durable)
                .transpose()
                .map_err(|_| ProductRunServiceError::Unavailable)?,
            remaining_work: record.remaining_work.clone(),
            interruption_cause: record.interruption_cause.clone(),
            candidate_actionable: Some(record.candidate_actionable),
        })
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
        let mut messages = self
            .messages
            .into_iter()
            .map(|message| {
                let role = ProductConversationRole::from_tag(message.role)
                    .ok_or(ProductRunServiceError::InvalidMessage)?;
                ProductConversationMessage::new(role, message.content)
                    .map_err(|_| ProductRunServiceError::InvalidMessage)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if messages.is_empty() {
            messages.push(
                ProductConversationMessage::new(ProductConversationRole::User, self.task.clone())
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            );
            if loaded_phase.terminal() && !self.summary.trim().is_empty() {
                messages.push(
                    ProductConversationMessage::new(
                        ProductConversationRole::Agent,
                        format!("{}: {}", status, self.summary),
                    )
                    .map_err(|_| ProductRunServiceError::InvalidMessage)?,
                );
            }
        }
        let conversation = SharedConversation::new(run_id, messages)?;
        let checkpoint = self.checkpoint.map(PersistedCheckpoint::into_checkpoint).transpose()?;
        let settlement = restore_settlement(checkpoint, self.settlement_cause)?;
        let resume = self
            .resume_state
            .map(|bytes| ProductRunResume::decode_durable(&bytes, &conversation.render()))
            .transpose()
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let invalid_lineage = checkpoint.is_some_and(|value| {
            value.identity().run_id() != run_id || value.identity().workspace_id() != workspace_id
        });
        let resume_mismatch =
            resume.as_ref().is_some_and(|value| Some(value.checkpoint()) != checkpoint.as_ref());
        if invalid_lineage || resume_mismatch {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let mut snapshot = ProductRunSnapshot::new(
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
        if let Some(deliverable) = self.deliverable {
            snapshot = snapshot.with_deliverable(deliverable.into_deliverable()?);
        }
        Ok(RunRecord {
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
            conversation,
            finding_state: self.finding_state,
            progress: self.progress.into_run(),
            checkpoint,
            settlement,
            resume,
            remaining_work: self.remaining_work,
            interruption_cause: self.interruption_cause,
            candidate_actionable: self.candidate_actionable.unwrap_or(true),
        })
    }
}

impl PersistedDeliverable {
    fn from_deliverable(value: &ProductDeliverable) -> Self {
        Self {
            workspace_path: value.workspace_path().to_owned(),
            changed_paths: value.changed_paths().to_vec(),
            successful_commands: value.successful_commands().to_vec(),
            run_instructions: value.run_instructions().to_owned(),
            qualification: Some(value.qualification().tag()),
            accepted: value.accepted(),
            commit_revision: value.commit_revision().to_owned(),
            export_path: value.export_path().to_owned(),
            discarded: value.discarded(),
        }
    }

    fn into_deliverable(self) -> Result<ProductDeliverable, ProductRunServiceError> {
        let qualification = self
            .qualification
            .map_or(Some(CandidateStage::Qualified), CandidateStage::from_tag)
            .ok_or(ProductRunServiceError::InvalidMessage)?;
        let mut value = ProductDeliverable::candidate(
            self.workspace_path,
            self.changed_paths,
            self.successful_commands,
            self.run_instructions,
            qualification,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if self.accepted {
            value = value.mark_accepted();
        }
        if !self.commit_revision.is_empty() {
            value = value
                .mark_committed(self.commit_revision)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        }
        if !self.export_path.is_empty() {
            value = value
                .mark_exported(self.export_path)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        }
        if self.discarded {
            value = value.mark_discarded();
        }
        Ok(value)
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

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
