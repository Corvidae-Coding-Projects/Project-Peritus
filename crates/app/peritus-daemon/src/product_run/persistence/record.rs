//! Conversion between live product-run ownership and persisted records.

use super::{
    Arc, AtomicBool, CancellationToken, PersistedCheckpoint, PersistedDeliverable,
    PersistedPreviewOperation, PersistedPreviewOutput, PersistedProgress, PersistedRecord,
    ProductProviderSelection, ProductRunPhase, ProductRunRequest, ProductRunResume,
    ProductRunServiceError, ProductRunSnapshot, ProviderProfileId, RunId, RunRecord, WorkspaceId,
    encode_workbench_result_value, interaction, restore_preview, restore_settlement,
};

const FORMAT_VERSION: u16 = 6;

impl PersistedRecord {
    pub(super) fn from_record(record: &RunRecord) -> Result<Self, ProductRunServiceError> {
        let snapshot = &record.snapshot;
        let providers = snapshot.providers();
        Ok(Self {
            format_version: FORMAT_VERSION,
            goal_resume: record.goal_resume.map(|operation| *operation.as_bytes()),
            interaction: interaction::PersistedInteraction::capture(&record.interaction),
            run_id: hex(snapshot.run_id().as_bytes()),
            workspace_id: hex(snapshot.workspace_id().as_bytes()),
            writer: hex(providers.writer().as_bytes()),
            reviewer: hex(providers.reviewer().as_bytes()),
            fixer: hex(providers.fixer().as_bytes()),
            phase: snapshot.phase().tag(),
            cycle: snapshot.cycle(),
            execution_task: record.request.execution_task().to_owned(),
            task: snapshot.task().to_owned(),
            status: snapshot.status().to_owned(),
            diff: snapshot.diff().to_owned(),
            gates: snapshot.gates().to_owned(),
            review: snapshot.review().to_owned(),
            summary: snapshot.summary().to_owned(),
            user_cancelled: record.user_cancelled,
            finding_state: record.finding_state.clone(),
            deliverable: snapshot.deliverable().map(PersistedDeliverable::from_deliverable),
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
            candidate_actionable: record.candidate_actionable,
            task_baseline_required: record.task_baseline_required,
            task_baseline: record.task_baseline.clone(),
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
                    stderr: record.preview.errors.get(launch).cloned().unwrap_or_default(),
                    truncated: record.preview.truncated.contains(launch),
                })
                .collect(),
        })
    }

    pub(super) fn into_record(self) -> Result<RunRecord, ProductRunServiceError> {
        self.into_record_with_context(None)
    }

    pub(super) fn into_record_with_context(
        self,
        governed: Option<&str>,
    ) -> Result<RunRecord, ProductRunServiceError> {
        // Resumable execution crosses its process boundary while decoding the continuation.
        // Terminal handoffs are reconciled later, after crash-safe commit/discard receipts have
        // been inspected against the exact identity that created them.
        if self.format_version != FORMAT_VERSION {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let interaction = self.interaction.restore()?;
        if self.goal_resume.is_some()
            && !matches!(
                interaction.workbench.intent(),
                peritus_product_runner::control::ControlIntent::StartGoal { .. }
            )
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let phase_tag = self.phase;
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
            &interaction,
        )?;
        {
            let operation = &interaction.workbench;
            let (peritus_product_runner::control::ControlIntent::StartExecution { run, .. }
            | peritus_product_runner::control::ControlIntent::StartGoal { run, .. }) =
                operation.intent()
            else {
                return Err(ProductRunServiceError::InvalidMessage);
            };
            if run != &run_id.into_bytes() || operation.workspace_bytes() != workspace_id.as_bytes()
            {
                return Err(ProductRunServiceError::InvalidMessage);
            }
        }
        let providers = ProductProviderSelection::new(
            profile(&self.writer)?,
            profile(&self.reviewer)?,
            profile(&self.fixer)?,
        );
        let request = ProductRunRequest::new(run_id, workspace_id, providers, self.execution_task)
            .and_then(|request| request.with_display_task(self.task.clone()))
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let loaded_phase =
            ProductRunPhase::from_tag(phase_tag).ok_or(ProductRunServiceError::InvalidMessage)?;
        let (phase, status) = if loaded_phase.terminal()
            && loaded_phase != ProductRunPhase::RecoveryRequired
        {
            (loaded_phase, self.status)
        } else if self.user_cancelled {
            (ProductRunPhase::Cancelled, "Run cancelled".to_owned())
        } else if loaded_phase == ProductRunPhase::RecoveryRequired {
            (loaded_phase, self.status)
        } else {
            (
                    ProductRunPhase::RecoveryRequired,
                    "Daemon restart interrupted this run; explicit retry is required to avoid replaying an indeterminate effect".to_owned(),
                )
        };
        let checkpoint = self.checkpoint.map(PersistedCheckpoint::into_checkpoint).transpose()?;
        let settlement = restore_settlement(checkpoint, self.settlement_cause)?;
        let governed = governed.ok_or(ProductRunServiceError::InvalidMessage)?;
        let resume = self
            .resume_state
            .map(|bytes| ProductRunResume::decode_durable_retained(&bytes, governed))
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
        let operation =
            super::super::operation::retained_execution(run_id, phase, &self.interruption_cause)?;
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
            operation,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if let Some(deliverable) = self.deliverable {
            snapshot = snapshot.with_deliverable(deliverable.into_deliverable()?);
        }
        if let Some(baseline) = &self.task_baseline {
            peritus_product_runner::ProductRunner::validate_task_baseline(baseline)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        }
        Ok(RunRecord {
            interaction,
            goal_resume: self
                .goal_resume
                .map(peritus_product_runner::control::OperationId::new)
                .transpose()
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            user_cancelled: self.user_cancelled,
            provider_cancellation: CancellationToken::new(),
            finding_state: self.finding_state,
            progress: self.progress.into_run(),
            checkpoint,
            settlement,
            resume,
            remaining_work: self.remaining_work,
            interruption_cause: self.interruption_cause,
            candidate_actionable: self.candidate_actionable,
            task_baseline_required: self.task_baseline_required,
            task_baseline: self.task_baseline,
            preview,
        })
    }
}

fn profile(value: &str) -> Result<ProviderProfileId, ProductRunServiceError> {
    ProviderProfileId::new(unhex(value)?).map_err(|_| ProductRunServiceError::InvalidMessage)
}

pub(super) fn hex(bytes: &[u8; 16]) -> String {
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
