//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationMessage, ProductConversationRole, ProductProviderSelection, ProductRunPhase,
    ProductRunRequest, ProductRunSnapshot, encode_workbench_result_value,
};
use peritus_product_runner::{ConversationView, ProductRunResume};
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::progress::RunProgress;
use super::{
    PreviewAggregate, PreviewOperationRecord, ProductRunServiceError, RunRecord,
    SharedConversation, filesystem, invalid,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

mod deliverable;
mod interaction;
mod preview;
mod progress;
use preview::restore_preview;
mod settlement;
mod workbench;
pub(super) use workbench::load_workbench_records;

use settlement::{PersistedCheckpoint, restore_settlement};

mod types;
use types::{
    PersistedDeliverable, PersistedMessage, PersistedPreviewOperation, PersistedPreviewOutput,
    PersistedProgress, PersistedRecord,
};

const MAX_PREVIEW_OPERATIONS: usize = 16_384;
const MAX_PREVIEW_OUTPUT_BYTES: usize = 4 * 1_024 * 1_024;

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let result = write_record(directory, record);
    if result.is_err()
        && let Some(options) = &record.interaction
    {
        options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
        record.cancelled.store(true, std::sync::atomic::Ordering::Release);
        let _ = record.provider_cancellation.cancel();
    }
    result
}

fn write_record(directory: &Path, record: &RunRecord) -> Result<(), ProductRunServiceError> {
    use std::io::Write as _;
    let workbench_directory;
    let directory =
        if record.interaction.as_ref().is_some_and(|options| options.workbench.is_some()) {
            workbench_directory = directory
                .parent()
                .ok_or(ProductRunServiceError::Unavailable)?
                .join("workbench-v1")
                .join("runs");
            fs::create_dir_all(&workbench_directory)
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            workbench_directory.as_path()
        } else {
            directory
        };
    let persisted = PersistedRecord::from_record(record)?;
    let bytes =
        serde_json::to_vec_pretty(&persisted).map_err(|_| ProductRunServiceError::Unavailable)?;
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|_| ProductRunServiceError::Unavailable)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    fs::rename(temporary, path).map_err(|_| ProductRunServiceError::Unavailable)?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| ProductRunServiceError::Unavailable)?;
    Ok(())
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
        if record.interaction.as_ref().is_some_and(|options| options.workbench.is_some()) {
            return Err(invalid(
                "workbench execution state cannot be loaded from the legacy generation",
            ));
        }
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
            interaction: record
                .interaction
                .as_ref()
                .map(interaction::PersistedInteraction::capture),
            run_id: hex(snapshot.run_id().as_bytes()),
            workspace_id: hex(snapshot.workspace_id().as_bytes()),
            writer: hex(providers.writer().as_bytes()),
            reviewer: hex(providers.reviewer().as_bytes()),
            fixer: hex(providers.fixer().as_bytes()),
            // Old readers reject interactive records instead of retrying them as build runs.
            phase: snapshot.phase().tag()
                + record
                    .interaction
                    .as_ref()
                    .map_or(0, |options| if options.workbench.is_some() { 200 } else { 100 }),
            cycle: snapshot.cycle(),
            task: snapshot.task().to_owned(),
            status: snapshot.status().to_owned(),
            diff: snapshot.diff().to_owned(),
            gates: snapshot.gates().to_owned(),
            review: snapshot.review().to_owned(),
            summary: snapshot.summary().to_owned(),
            user_cancelled: record.user_cancelled,
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
            preview_page: record
                .preview
                .page
                .as_ref()
                .map(encode_workbench_result_value)
                .transpose()
                .map_err(|_| ProductRunServiceError::Unavailable)?,
            preview_operations: record
                .preview
                .operations
                .iter()
                .map(|(operation, value)| PersistedPreviewOperation {
                    operation: operation.into_bytes(),
                    fingerprint: value.fingerprint.into_bytes(),
                    accepted_revision: value.accepted_revision,
                    result_sequence: value.result_sequence,
                    completed_sequence: value.completed_sequence,
                })
                .collect(),
            preview_outputs: record
                .preview
                .outputs
                .iter()
                .map(|(launch, stdout)| PersistedPreviewOutput {
                    launch: launch.into_bytes(),
                    stdout: stdout.clone(),
                })
                .collect(),
        })
    }

    fn into_record(self) -> Result<RunRecord, ProductRunServiceError> {
        self.into_record_with_context(None)
    }

    fn into_record_with_context(
        self,
        governed: Option<&str>,
    ) -> Result<RunRecord, ProductRunServiceError> {
        let interaction =
            self.interaction.map(interaction::PersistedInteraction::restore).transpose()?;
        let phase_tag = if let Some(options) = &interaction {
            self.phase
                .checked_sub(if options.workbench.is_some() { 200 } else { 100 })
                .ok_or(ProductRunServiceError::InvalidMessage)?
        } else {
            self.phase
        };
        let run_id =
            RunId::new(unhex(&self.run_id)?).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let workspace_id = WorkspaceId::new(unhex(&self.workspace_id)?)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let preview = restore_preview(
            self.preview_page,
            self.preview_operations,
            self.preview_outputs,
            run_id,
            workspace_id,
            interaction.as_ref(),
        )?;
        if let Some(operation) = interaction.as_ref().and_then(|options| options.workbench.as_ref())
        {
            let peritus_product_runner::control::ControlIntent::StartExecution { run, .. } =
                operation.intent()
            else {
                return Err(ProductRunServiceError::InvalidMessage);
            };
            if *run != run_id.into_bytes() || operation.workspace_bytes() != workspace_id.as_bytes()
            {
                return Err(ProductRunServiceError::InvalidMessage);
            }
        }
        let providers = ProductProviderSelection::new(
            profile(&self.writer)?,
            profile(&self.reviewer)?,
            profile(&self.fixer)?,
        );
        let request = ProductRunRequest::new(run_id, workspace_id, providers, self.task.clone())
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let loaded_phase =
            ProductRunPhase::from_tag(phase_tag).ok_or(ProductRunServiceError::InvalidMessage)?;
        let (phase, status) = if self.user_cancelled {
            (ProductRunPhase::Cancelled, "Run cancelled".to_owned())
        } else if loaded_phase.terminal() {
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
        if interaction.as_ref().is_some_and(|state| {
            state.workbench.is_none() && state.incorporated > conversation.revision()
        }) {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let checkpoint = self.checkpoint.map(PersistedCheckpoint::into_checkpoint).transpose()?;
        let settlement = restore_settlement(checkpoint, self.settlement_cause)?;
        let resume = self
            .resume_state
            .map(|bytes| {
                ProductRunResume::decode_durable(&bytes, governed.unwrap_or(&conversation.render()))
            })
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
            interaction,
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            user_cancelled: self.user_cancelled,
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
            preview,
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

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
