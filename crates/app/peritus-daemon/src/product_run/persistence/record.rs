//! Conversion between live product-run ownership and persisted records.

use super::{
    Arc, AtomicBool, CancellationToken, ContinuationSource, PersistedCheckpoint,
    PersistedContinuationSource, PersistedDeliverable,
    PersistedPreviewOperation, PersistedPreviewOutput, PersistedProgress, PersistedRecord,
    PersistedRejectedFindingUpdate, RejectedFindingUpdate,
    ProductProviderSelection, ProductRunPhase, ProductRunRequest,
    ProductRunServiceError, ProductRunSnapshot, ProviderProfileId, RunId, RunRecord, WorkspaceId,
    continuation, encode_workbench_result_value, interaction, restore_preview, restore_settlement,
    FindingBodyStore, OpaqueResume,
    LegacyAssessment, PersistedObligation, SettlementObligation,
};

const FORMAT_VERSION: u16 = 8;
const FINDING_BODY_FORMAT_VERSION: u16 = 9;
const REVIEW_ARTIFACT_FORMAT_VERSION: u16 = 10;
const HANDOFF_FORMAT_VERSION: u16 = 11;
const PUBLICATION_FORMAT_VERSION: u16 = 12;
const PAGED_RESUME_FORMAT_VERSION: u16 = 8;
const TELEMETRY_FORMAT_VERSION: u16 = 7;
const LEGACY_FORMAT_VERSION: u16 = 6;

impl PersistedRecord {
    pub(super) fn publication_head(
        &self,
        run: RunId,
        source_digest: peritus_types::Sha256Digest,
    ) -> Result<(u64, peritus_types::Sha256Digest), ProductRunServiceError> {
        let embedded = RunId::new(unhex(&self.run_id)?)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if embedded != run {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        match self.format_version {
            PUBLICATION_FORMAT_VERSION
                if self.record_revision != 0 && self.record_lineage_root != [0; 32] =>
            {
                Ok((
                    self.record_revision,
                    peritus_types::Sha256Digest::new(self.record_lineage_root),
                ))
            }
            LEGACY_FORMAT_VERSION
            | TELEMETRY_FORMAT_VERSION
            | FORMAT_VERSION
            | FINDING_BODY_FORMAT_VERSION
            | REVIEW_ARTIFACT_FORMAT_VERSION
            | HANDOFF_FORMAT_VERSION
                if self.record_revision == 0
                    && self.record_lineage_root == [0; 32]
                    && self.settlement_obligation.is_none() =>
            {
                Ok((
                    1,
                    legacy_lineage_root(self.format_version, run, source_digest),
                ))
            }
            _ => Err(ProductRunServiceError::InvalidMessage),
        }
    }

    pub(super) fn from_record(
        record: &RunRecord,
        resume_root: Option<super::PersistedResumeRoot>,
    ) -> Result<Self, ProductRunServiceError> {
        if record.attempt_sequence == 0
            || record.review_artifact_migration_version
                > super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
            || (record.review_artifact_migration_version
                == super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
                && (!record.finding_catalog.review_artifacts_externalized
                    || record.opaque_resume.is_some()
                    || record.resume.as_ref().is_some_and(|resume| {
                        !resume.review_artifacts_externalized()
                    })))
        {
            return Err(ProductRunServiceError::internal(
                "persist product finding migration marker",
                "the record's migration marker does not match its review-artifact authority",
            ));
        }
        if record.finding_catalog.head_digest
            != peritus_codec::sha256(record.finding_state.as_bytes())
        {
            return Err(ProductRunServiceError::internal(
                "persist product finding state",
                "the immutable source catalog does not match the finding ledger head",
            ));
        }
        if record.rejected_finding_update.as_ref().is_some_and(|rejected| {
            rejected.accepted_head != record.finding_catalog.head_digest
                || rejected.accepted_head == rejected.rejected_head
                || rejected.rejected_head
                    != peritus_codec::sha256(rejected.rejected_finding_state.as_bytes())
                || rejected.rejected_bytes
                    != u64::try_from(rejected.rejected_finding_state.len()).unwrap_or(u64::MAX)
                || rejected.error.trim().is_empty()
                || !(1..=8).contains(&rejected.phase)
        }) {
            return Err(ProductRunServiceError::internal(
                "persist rejected product finding update",
                "the rejected finding diagnostic does not bind its retained evidence",
            ));
        }
        let snapshot = &record.snapshot;
        let providers = snapshot.providers();
        let (resume_state, resume_root) = match (
            &record.resume,
            &record.opaque_resume,
            resume_root,
        ) {
            (Some(_), None, Some(root)) => (None, Some(root)),
            (Some(resume), None, None) => {
                let mut bytes = Vec::new();
                resume.encode_durable_into(&mut bytes).map_err(|error| {
                    ProductRunServiceError::internal(
                        "capture retained continuation",
                        error.to_string(),
                    )
                })?;
                (Some(bytes), None)
            }
            (Some(_), Some(_), _) => {
                return Err(ProductRunServiceError::internal(
                    "persist retained continuation",
                    "decoded and opaque continuation authority cannot coexist",
                ));
            }
            (None, Some(OpaqueResume::Inline(bytes)), None) => (Some(bytes.clone()), None),
            (None, Some(OpaqueResume::Root(root)), None) => (None, Some(root.clone())),
            (None, Some(_), Some(_)) => {
                return Err(ProductRunServiceError::internal(
                    "persist retained continuation",
                    "opaque continuation unexpectedly published a replacement page root",
                ));
            }
            (None, None, None) => (None, None),
            (None, None, Some(_)) => {
                return Err(ProductRunServiceError::internal(
                    "persist retained continuation",
                    "a page root was published without decoded continuation state",
                ));
            }
        };
        Ok(Self {
            format_version: if record.record_revision == 0 {
                HANDOFF_FORMAT_VERSION
            } else {
                PUBLICATION_FORMAT_VERSION
            },
            record_revision: record.record_revision,
            record_lineage_root: record.record_lineage_root.into_bytes(),
            settlement_obligation: record
                .settlement_obligation
                .as_ref()
                .map(PersistedObligation::capture),
            attempt_sequence: record.attempt_sequence,
            handoff_sequence: record.handoff_sequence,
            review_artifact_migration_version: record.review_artifact_migration_version,
            handoff_recovery_pending: record.handoff_recovery_pending,
            rejected_finding_update: record.rejected_finding_update.as_ref().map(|rejected| {
                PersistedRejectedFindingUpdate {
                    accepted_head: rejected.accepted_head.into_bytes(),
                    rejected_head: rejected.rejected_head.into_bytes(),
                    rejected_bytes: rejected.rejected_bytes,
                    rejected_finding_state: rejected.rejected_finding_state.clone(),
                    phase: rejected.phase,
                    cycle: rejected.cycle,
                    status: rejected.status.clone(),
                    diff: rejected.diff.clone(),
                    gates: rejected.gates.clone(),
                    review: rejected.review.clone(),
                    summary: rejected.summary.clone(),
                    error: rejected.error.clone(),
                }
            }),
            attempt_admission: record.attempt_admission.map(|operation| *operation.as_bytes()),
            continuation_admissions: record
                .continuation_admissions
                .iter()
                .map(|operation| *operation.as_bytes())
                .collect(),
            continuation_sources: record
                .continuation_sources
                .iter()
                .map(|source| PersistedContinuationSource {
                    operation: source.operation.into_bytes(),
                    revision: source.revision,
                    generation: source.generation,
                    settled: source.settled,
                })
                .collect(),
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
            resume_state,
            resume_root,
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
        self.into_record_with_storage(None, None, None)
    }

    pub(super) fn into_record_with_context(
        self,
        governed: Option<&str>,
    ) -> Result<RunRecord, ProductRunServiceError> {
        self.into_record_with_storage(governed, None, None)
    }

    pub(super) fn into_record_with_storage(
        mut self,
        governed: Option<&str>,
        workbench_root: Option<&std::path::Path>,
        source_digest: Option<peritus_types::Sha256Digest>,
    ) -> Result<RunRecord, ProductRunServiceError> {
        // Resumable execution crosses its process boundary while decoding the continuation.
        // Terminal handoffs are reconciled later, after crash-safe commit/discard receipts have
        // been inspected against the exact identity that created them.
        let source_format = self.format_version;
        let source_digest = source_digest.unwrap_or_else(|| {
            serde_json::to_vec(&self)
                .map_or(peritus_types::Sha256Digest::new([0; 32]), |bytes| {
                    peritus_codec::sha256(&bytes)
                })
        });
        if !matches!(
            self.format_version,
            LEGACY_FORMAT_VERSION
                | TELEMETRY_FORMAT_VERSION
                | FORMAT_VERSION
                | FINDING_BODY_FORMAT_VERSION
                | REVIEW_ARTIFACT_FORMAT_VERSION
                | HANDOFF_FORMAT_VERSION
                | PUBLICATION_FORMAT_VERSION
        )
            || self.attempt_sequence == 0
            || self.review_artifact_migration_version
                > super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
            || (self.format_version < HANDOFF_FORMAT_VERSION
                && (self.attempt_sequence != 1
                    || self.handoff_sequence != 0
                    || self.review_artifact_migration_version != 0
                    || self.handoff_recovery_pending
                    || self.rejected_finding_update.is_some()))
            || (self.format_version < PUBLICATION_FORMAT_VERSION
                && (self.record_revision != 0
                    || self.record_lineage_root != [0; 32]
                    || self.settlement_obligation.is_some()))
            || (self.format_version == PUBLICATION_FORMAT_VERSION
                && (self.record_revision == 0 || self.record_lineage_root == [0; 32]))
            || self.rejected_finding_update.as_ref().is_some_and(|rejected| {
                rejected.rejected_bytes
                    != u64::try_from(rejected.rejected_finding_state.len()).unwrap_or(u64::MAX)
                    || rejected.rejected_head
                        != peritus_codec::sha256(rejected.rejected_finding_state.as_bytes())
                            .into_bytes()
                    || rejected.error.trim().is_empty()
                    || rejected.accepted_head == rejected.rejected_head
                    || !(1..=8).contains(&rejected.phase)
            })
            || (self.format_version >= TELEMETRY_FORMAT_VERSION
                && self.progress.resource_telemetry.is_none())
            || (self.format_version == LEGACY_FORMAT_VERSION
                && self.progress.resource_telemetry.is_some())
            || !self.progress.provider_request_is_valid()
            || (self.format_version < PAGED_RESUME_FORMAT_VERSION
                && self.resume_root.is_some())
            || (self.resume_state.is_some() && self.resume_root.is_some())
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let interaction = self.interaction.restore()?;
        if self.attempt_admission.is_some()
            && !matches!(
                interaction.workbench.intent(),
                peritus_product_runner::control::ControlIntent::StartGoal { .. }
            )
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        if !self.continuation_admissions.is_empty()
            && !matches!(
                interaction.workbench.intent(),
                peritus_product_runner::control::ControlIntent::StartExecution { .. }
            )
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let phase_tag = self.phase;
        let run_id =
            RunId::new(unhex(&self.run_id)?).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let (record_revision, record_lineage_root) =
            if source_format == PUBLICATION_FORMAT_VERSION {
                (
                    self.record_revision,
                    peritus_types::Sha256Digest::new(self.record_lineage_root),
                )
            } else {
                (
                    1,
                    legacy_lineage_root(source_format, run_id, source_digest),
                )
            };
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
        let settlement_obligation = match (
            source_format,
            self.settlement_obligation.take(),
            loaded_phase.terminal(),
        ) {
            (PUBLICATION_FORMAT_VERSION, Some(obligation), _) => {
                let obligation = obligation.restore()?;
                if !obligation.binds(run_id, self.attempt_sequence) {
                    return Err(ProductRunServiceError::InvalidMessage);
                }
                Some(obligation)
            }
            (PUBLICATION_FORMAT_VERSION, None, true) => Some(
                SettlementObligation::LegacyAssessmentRequired(LegacyAssessment::new(
                    source_format,
                    source_digest,
                    run_id,
                    self.attempt_sequence,
                    self.handoff_sequence,
                    interaction.incorporated,
                    source_digest,
                )),
            ),
            (_, None, true) => Some(SettlementObligation::LegacyAssessmentRequired(
                LegacyAssessment::new(
                    source_format,
                    source_digest,
                    run_id,
                    self.attempt_sequence,
                    self.handoff_sequence,
                    interaction.incorporated,
                    source_digest,
                ),
            )),
            (_, None, false) => None,
            (_, Some(_), _) => return Err(ProductRunServiceError::InvalidMessage),
        };
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
        let (resume, opaque_resume) = match (self.resume_state, self.resume_root) {
            (Some(bytes), None) => {
                let governed = governed.ok_or(ProductRunServiceError::InvalidMessage)?;
                match peritus_product_runner::ProductRunResume::decode_durable_retained(
                    &bytes,
                    governed,
                ) {
                    Ok(resume) => (Some(resume), None),
                    Err(error) => {
                        crate::diagnostic::report(&format!(
                            "peritusd: retained an unreadable inline continuation for run {run_id}: {error}"
                        ));
                        (None, Some(OpaqueResume::Inline(bytes)))
                    }
                }
            }
            (None, Some(root)) => {
                let governed = governed.ok_or(ProductRunServiceError::InvalidMessage)?;
                let checkpoint =
                    checkpoint.as_ref().ok_or(ProductRunServiceError::InvalidMessage)?;
                if !root.binds_checkpoint(checkpoint) {
                    return Err(ProductRunServiceError::InvalidMessage);
                }
                if !root.matches_checkpoint(checkpoint) {
                    (None, Some(OpaqueResume::Root(root)))
                } else if let Some(workbench_root) = workbench_root {
                    match continuation::restore(workbench_root, &root, governed) {
                        Ok(resume) => (Some(resume), None),
                        Err(error) => {
                            crate::diagnostic::report(&format!(
                                "peritusd: retained an unreadable paged continuation for run {run_id}: {error}"
                            ));
                            (None, Some(OpaqueResume::Root(root)))
                        }
                    }
                } else {
                    (None, Some(OpaqueResume::Root(root)))
                }
            }
            (None, None) => (None, None),
            (Some(_), Some(_)) => return Err(ProductRunServiceError::InvalidMessage),
        };
        let invalid_lineage = checkpoint.is_some_and(|value| {
            value.identity().run_id() != run_id || value.identity().workspace_id() != workspace_id
        });
        let resume_mismatch =
            resume.as_ref().is_some_and(|value| Some(value.checkpoint()) != checkpoint.as_ref());
        if invalid_lineage || resume_mismatch {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let resume_decode_pending = opaque_resume.is_some();
        let finding_catalog = FindingBodyStore::catalog(&self.finding_state)?;
        if self.rejected_finding_update.as_ref().is_some_and(|rejected| {
            peritus_types::Sha256Digest::new(rejected.accepted_head)
                != finding_catalog.head_digest
        }) {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        if self.review_artifact_migration_version
            == super::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
            && (!finding_catalog.review_artifacts_externalized
                || opaque_resume.is_some()
                || resume
                    .as_ref()
                    .is_some_and(|resume| !resume.review_artifacts_externalized()))
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let (phase, status) = if resume_decode_pending {
            (
                ProductRunPhase::RecoveryRequired,
                "The exact continuation is retained but requires a compatible decoder before this run can continue".to_owned(),
            )
        } else {
            (phase, status)
        };
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
        let continuation_admissions = self
            .continuation_admissions
            .into_iter()
            .map(peritus_product_runner::control::OperationId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if continuation_admissions
            .iter()
            .enumerate()
            .any(|(index, operation)| continuation_admissions[..index].contains(operation))
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let continuation_sources = self
            .continuation_sources
            .into_iter()
            .map(|source| {
                Ok(ContinuationSource {
                    operation: peritus_product_runner::control::OperationId::new(
                        source.operation,
                    )?,
                    revision: source.revision,
                    generation: source.generation,
                    settled: source.settled,
                })
            })
            .collect::<Result<Vec<_>, peritus_product_runner::control::ControlError>>()
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if continuation_sources.iter().filter(|source| !source.settled).count() > 1
            || continuation_sources.iter().enumerate().any(|(index, source)| {
            source.revision == 0
                || source.generation == 0
                || !continuation_admissions.contains(&source.operation)
                || continuation_sources[..index]
                    .iter()
                    .any(|prior| prior.operation == source.operation)
        })
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        Ok(RunRecord {
            attempt_sequence: self.attempt_sequence,
            handoff_sequence: self.handoff_sequence,
            record_revision,
            record_lineage_root,
            durable_record_revision: record_revision,
            durable_lineage_root: record_lineage_root,
            durable_canonical_digest: source_digest,
            settlement_obligation,
            review_artifact_migration_version: self.review_artifact_migration_version,
            handoff_recovery_pending: self.handoff_recovery_pending,
            rejected_finding_update: self.rejected_finding_update.map(|rejected| {
                RejectedFindingUpdate {
                    accepted_head: peritus_types::Sha256Digest::new(rejected.accepted_head),
                    rejected_head: peritus_types::Sha256Digest::new(rejected.rejected_head),
                    rejected_bytes: rejected.rejected_bytes,
                    rejected_finding_state: rejected.rejected_finding_state,
                    phase: rejected.phase,
                    cycle: rejected.cycle,
                    status: rejected.status,
                    diff: rejected.diff,
                    gates: rejected.gates,
                    review: rejected.review,
                    summary: rejected.summary,
                    error: rejected.error,
                }
            }),
            interaction,
            attempt_admission: self
                .attempt_admission
                .map(peritus_product_runner::control::OperationId::new)
                .transpose()
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            continuation_admissions,
            continuation_sources,
            request,
            snapshot,
            cancelled: Arc::new(AtomicBool::new(false)),
            control_cancellation: peritus_journal::JournalCancellation::new(),
            user_cancelled: self.user_cancelled,
            provider_cancellation: CancellationToken::new(),
            finding_state: self.finding_state,
            finding_catalog,
            progress: self.progress.into_run(),
            checkpoint,
            settlement,
            resume,
            opaque_resume,
            remaining_work: self.remaining_work,
            interruption_cause: self.interruption_cause,
            candidate_actionable: self.candidate_actionable && !resume_decode_pending,
            task_baseline_required: self.task_baseline_required,
            task_baseline: self.task_baseline,
            preview,
        })
    }
}

fn legacy_lineage_root(
    format: u16,
    run: RunId,
    source_digest: peritus_types::Sha256Digest,
) -> peritus_types::Sha256Digest {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-product-run-legacy-baseline-v1\0");
    hasher.update(format.to_be_bytes());
    hasher.update(run.as_bytes());
    hasher.update(source_digest.into_bytes());
    peritus_types::Sha256Digest::new(hasher.finalize().into())
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
