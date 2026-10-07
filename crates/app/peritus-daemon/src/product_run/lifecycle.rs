//! Cancellation, restart recovery, and retry lifecycle for product runs.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

mod shutdown;

use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot, WorkbenchExecutionSettings};
use peritus_provider_core::CancellationToken;
use peritus_types::{RunId, Sha256Digest};

use super::{
    MutationDisposition, PreparedRetry, PreparedRunLaunch, ProductRunService,
    ProductRunServiceError, RunMutationKind,
};
use super::{snapshot::replace_snapshot, snapshot::workspace_has_active_run};

pub(super) enum RetryAdmission {
    Goal(peritus_product_runner::control::ControlOperation),
    Continuation {
        operation: peritus_product_runner::control::ControlOperation,
        settings: WorkbenchExecutionSettings,
    },
}

impl ProductRunService {
    /// Publishes every accepted user-cancellation generation before atomically retiring an owner.
    ///
    /// The records-to-cancellation-registry lock order matches admission and cancellation. Both
    /// locks are released before immutable publication; the retained owner remains the only
    /// same-run publisher while storage retries.
    pub(super) fn seal_run_owner(
        &self,
        run_id: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<(), ProductRunServiceError> {
        let Some(generation) = self.seal_run_cancellation(run_id, attempt)? else {
            return Ok(());
        };
        let kind = if self.inner.control_shutdown.is_cancelled() {
            RunMutationKind::ShutdownSettlement
        } else {
            RunMutationKind::Cancellation
        };
        let input = lifecycle_input(b"seal-owner-cancellation");
        let (publication, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            kind,
            input,
            MutationDisposition::DurabilityRequired,
            |record| {
                let prior_sequence = record.handoff_sequence;
                let attempt_sequence = record.attempt_sequence;
                let mut cancellation = record.clone();
                cancellation.handoff_sequence = prior_sequence.checked_add(1).ok_or_else(|| {
                    ProductRunServiceError::internal(
                        "publish final owner cancellation handoff",
                        "handoff sequence overflow",
                    )
                })?;
                cancellation.user_cancelled = true;
                cancellation.snapshot = replace_snapshot(
                    &cancellation.snapshot,
                    ProductRunPhase::Cancelled,
                    "Run cancelled",
                    "Cancelled before the retained run owner retired",
                )?;
                cancellation.handoff_recovery_pending = true;
                record.user_cancelled = true;
                record.snapshot = cancellation.snapshot.clone();
                record.handoff_recovery_pending = true;
                Ok((
                    cancellation,
                    prior_sequence,
                    attempt_sequence,
                ))
            },
        )?;
        self.await_run_durable(ticket)?;
        let (mut cancellation, prior_sequence, attempt_sequence) = publication;
        let handoff_cancellation = if self.inner.control_shutdown.is_cancelled() {
            &self.inner.control_shutdown
        } else {
            &self.inner.control_reconciliation
        };
        super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &cancellation,
            super::persistence::HandoffKind::Terminal,
            handoff_cancellation,
        )?;
        cancellation.handoff_recovery_pending = false;
        let (_, ticket) = self.mutate_run(
            run_id,
            Some(attempt),
            if self.inner.control_shutdown.is_cancelled() {
                RunMutationKind::ShutdownSettlement
            } else {
                RunMutationKind::Recovery
            },
            lifecycle_input(b"install-owner-cancellation-handoff"),
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || !record.user_cancelled
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                cancellation.interaction = record.interaction.clone();
                cancellation.preview = record.preview.clone();
                install_handoff_record(record, cancellation);
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)?;
        self.acknowledge_user_cancellation(run_id, attempt, generation)?;
        match self.seal_run_cancellation(run_id, attempt)? {
            None => Ok(()),
            Some(_) => Err(ProductRunServiceError::InvalidState),
        }
    }

    pub(super) fn cancel(
        &self,
        run_id: RunId,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let identity = self.capture_run_identity(run_id)?;
        let (phase, handoff_recovery_pending) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?;
            (record.snapshot.phase(), record.handoff_recovery_pending)
        };
        let owner_request = if handoff_recovery_pending
            || !phase.terminal()
            || matches!(phase, ProductRunPhase::RecoveryRequired | ProductRunPhase::Cancelled)
        {
            self.request_user_cancellation_for_attempt(run_id, &identity.cancelled)?
        } else {
            None
        };
        let cancellation_requested = self.user_cancellation_requested(run_id);
        let mutation = self.mutate_run(
            run_id,
            Some(&identity.cancelled),
            RunMutationKind::Cancellation,
            lifecycle_input(b"user-cancellation"),
            MutationDisposition::DurabilityRequired,
            |record| {
                let phase = record.snapshot.phase();
                let mut cancel_tokens = false;
                let mut acknowledge = true;
                if let Some((_, false)) = owner_request
                    && !record.handoff_recovery_pending
                {
                    record.user_cancelled = true;
                    record.snapshot = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled before the retained run owner launched",
                    )?;
                    cancel_tokens = true;
                } else if phase == ProductRunPhase::Queued && !record.handoff_recovery_pending {
                    record.user_cancelled = true;
                    record.snapshot = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled before execution started",
                    )?;
                    cancel_tokens = true;
                } else if owner_request.is_some()
                    && (record.handoff_recovery_pending
                        || matches!(
                            phase,
                            ProductRunPhase::RecoveryRequired | ProductRunPhase::Cancelled
                        ))
                {
                    record.user_cancelled = true;
                    record.snapshot = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while durable recovery completes",
                    )?;
                    acknowledge = false;
                } else if phase == ProductRunPhase::WaitingForUser {
                    record.snapshot = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while waiting for your reply",
                    )?;
                    record.interaction.append(
                        peritus_app_protocol::ProductActivityKind::Status,
                        "Run cancelled",
                        "Cancelled while waiting for your reply",
                    )?;
                    cancel_tokens = true;
                } else if phase.terminal() {
                    if phase != ProductRunPhase::Cancelled
                        || !cancellation_requested
                    {
                        return Err(ProductRunServiceError::InvalidState);
                    }
                } else {
                    record.user_cancelled = true;
                    record.snapshot = replace_snapshot(
                        &record.snapshot,
                        phase,
                        "Cancellation requested",
                        record.snapshot.summary(),
                    )?;
                    cancel_tokens = true;
                }
                Ok((
                    record.snapshot.clone(),
                    cancel_tokens,
                    acknowledge,
                    record.control_cancellation.clone(),
                    record.provider_cancellation.clone(),
                ))
            },
        );
        let (completion, ticket) = match mutation {
            Ok(value) => value,
            Err(error) => {
                if owner_request.is_some_and(|(_, launched)| !launched) {
                    self.dismiss_user_cancellation_for_attempt(run_id, &identity.cancelled);
                    self.retire_run_cancellation(run_id, &identity.cancelled);
                }
                return Err(error);
            }
        };
        self.await_run_durable(ticket)?;
        let (snapshot, cancel_tokens, acknowledge, control, provider) = completion;
        if acknowledge
            && let Some((generation, _)) = owner_request
        {
            self.acknowledge_user_cancellation(run_id, &identity.cancelled, generation)?;
        }
        if cancel_tokens {
            identity.cancelled.store(true, Ordering::Release);
            let _ = provider.cancel();
            control.cancel();
        }
        Ok(snapshot)
    }

    pub(super) async fn retry(
        &self,
        run_id: RunId,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.retry_admitted(run_id, None).await
    }

    pub(super) async fn retry_admitted(
        &self,
        run_id: RunId,
        admission: Option<RetryAdmission>,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let service = self.clone();
        let prepared = Self::await_blocking_owner("prepare durable product-run retry", move || {
            service.prepare_retry_admitted(run_id, admission.as_ref())
        })
        .await?;
        let prepared = match prepared {
            PreparedRetry::Existing(snapshot) => return Ok(snapshot),
            PreparedRetry::Launch(prepared) => prepared,
        };
        let snapshot = prepared.snapshot.clone();
        self.launch_prepared(prepared).await?;
        Ok(snapshot)
    }

    fn prepare_retry_admitted(
        &self,
        run_id: RunId,
        admission: Option<&RetryAdmission>,
    ) -> Result<PreparedRetry, ProductRunServiceError> {
        let (records_view, mut record_view) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone();
            (records.clone(), record)
        };
        let workspace_id = record_view.request.workspace_id();
        let mut attempt_interaction = record_view.interaction.clone();
        let (explicit_goal, continuation_source) = match admission {
            Some(RetryAdmission::Goal(operation)) => {
                if !self.goal_resume_pending(&record_view, operation)? {
                    return Ok(PreparedRetry::Existing(record_view.snapshot.clone()));
                }
                (true, None)
            }
            Some(RetryAdmission::Continuation { operation, settings }) => {
                let Some(source) =
                    self.continuation_pending(&record_view, operation, settings)?
                else {
                    return Ok(PreparedRetry::Existing(record_view.snapshot.clone()));
                };
                attempt_interaction.mode = settings.mode();
                attempt_interaction.models = settings.models().clone();
                (false, Some(source))
            }
            None => (false, None),
        };
        let explicit_continuation = continuation_source.is_some();
        let retained_continuation = record_view
            .continuation_sources
            .iter()
            .rev()
            .find(|source| !source.settled)
            .copied();
        if admission.is_none()
            && let Some(source) = retained_continuation
            && self.continuation_owner_retained(run_id, source.operation)?
        {
            return Ok(PreparedRetry::Existing(record_view.snapshot.clone()));
        }
        let owned_continuation = continuation_source.or(retained_continuation);
        let recovering_continuation = retained_continuation.is_some_and(|retained| {
            continuation_source.is_none_or(|source| source.operation == retained.operation)
        }) || continuation_source.is_some_and(|source| {
            record_view.continuation_admissions.contains(&source.operation)
        });
        if workspace_has_active_run(&records_view, workspace_id, Some(run_id)) {
            return Err(ProductRunServiceError::InvalidState);
        }
        super::deliverable::discard::workspace_available(
            &self.inner.directory,
            &records_view,
            workspace_id,
        )?;
        let pending_chat = matches!(
            record_view.snapshot.phase(),
            ProductRunPhase::WaitingForUser | ProductRunPhase::Complete
        ) && self.pending_record_input(&record_view)?;
        let explicit_idle = (explicit_goal || explicit_continuation)
            && matches!(
                record_view.snapshot.phase(),
                ProductRunPhase::WaitingForUser | ProductRunPhase::Complete
            );
        if !(record_view.snapshot.phase().retryable()
            || pending_chat
            || explicit_idle
            || recovering_continuation)
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        self.validate_workspace_mode(workspace_id, attempt_interaction.mode)?;
        let providers = self.resolve_selected_providers(
            record_view.request.providers(),
            &attempt_interaction,
        )?;
        let workspace_root = self
            .inner
            .workspaces
            .get(&workspace_id)
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let control_cancellation = peritus_journal::JournalCancellation::new();
        let provider_cancellation = CancellationToken::new();
        if !record_view.handoff_recovery_pending
            && record_view.review_artifact_migration_version
            < super::persistence::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION
        {
            self.ensure_run_admission()?;
            if self.run_owner_active(run_id)? {
                return Err(ProductRunServiceError::InvalidState);
            }
            let mut migrated = record_view.clone();
            migrated.cancelled = Arc::clone(&cancelled);
            migrated.control_cancellation = control_cancellation.clone();
            migrated.provider_cancellation = provider_cancellation.clone();
            migrated.handoff_recovery_pending = true;
            let migration = self
                .inner
                .finding_bodies
                .migrate_record(&mut migrated)
                .and_then(|_| {
                    migrated.review_artifact_migration_version =
                        super::persistence::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION;
                    migrated.handoff_sequence =
                        migrated.handoff_sequence.checked_add(1).ok_or_else(|| {
                            ProductRunServiceError::internal(
                                "publish retry-owned review artifact migration",
                                "handoff sequence overflow",
                            )
                        })?;
                    super::persistence::write_handoff_retrying(
                        &self.inner.directory,
                        &migrated,
                        super::persistence::HandoffKind::Migration,
                        &self.inner.control_shutdown,
                    )
                });
            if let Err(error) = migration {
                let detail = error.describe();
                let (_, ticket) = self.mutate_run(
                    run_id,
                    Some(&record_view.cancelled),
                    RunMutationKind::Recovery,
                    peritus_codec::sha256(detail.as_bytes()),
                    MutationDisposition::DurabilityRequired,
                    |record| {
                        if record.attempt_sequence != record_view.attempt_sequence
                            || record.handoff_sequence != record_view.handoff_sequence
                        {
                            return Err(ProductRunServiceError::InvalidState);
                        }
                    record.handoff_recovery_pending = false;
                    record.candidate_actionable = false;
                        record.interruption_cause.clone_from(&detail);
                        record.snapshot = replace_snapshot(
                        &record.snapshot,
                        ProductRunPhase::RecoveryRequired,
                        "Product finding bodies require durable migration before retry",
                            &detail,
                        )?;
                        Ok(())
                    },
                )?;
                self.await_run_durable(ticket)?;
                return Err(error);
            }
            migrated.handoff_recovery_pending = false;
            let (migrated_cancelled, ticket) = self.mutate_run(
                run_id,
                Some(&record_view.cancelled),
                RunMutationKind::Recovery,
                lifecycle_input(b"install-review-artifact-migration"),
                MutationDisposition::DurabilityRequired,
                |record| {
                if record.attempt_sequence != record_view.attempt_sequence
                    || record.handoff_sequence != record_view.handoff_sequence
                    || record.interaction.workbench != record_view.interaction.workbench
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                migrated.user_cancelled |= record.user_cancelled;
                let migrated_cancelled = migrated.user_cancelled;
                if migrated.user_cancelled {
                    migrated.snapshot = replace_snapshot(
                        &migrated.snapshot,
                        ProductRunPhase::Cancelled,
                        "Run cancelled",
                        "Cancelled while durable review-artifact migration completed",
                    )
                    .unwrap_or_else(|_| record.snapshot.clone());
                }
                migrated.interaction = record.interaction.clone();
                migrated.preview = record.preview.clone();
                install_handoff_record(record, migrated);
                Ok(migrated_cancelled)
                },
            )?;
            self.await_run_durable(ticket)?;
            if migrated_cancelled {
                cancelled.store(true, Ordering::Release);
            }
            record_view = {
                let records = self
                    .inner
                    .records
                    .read()
                    .map_err(|_| ProductRunServiceError::Unavailable)?;
                records.get(&run_id).ok_or(ProductRunServiceError::NotFound)?.clone()
            };
        }
        if cancelled.load(Ordering::Acquire) {
            let snapshot = self.publish_retry_migration_cancellation(run_id, &cancelled)?;
            return Ok(PreparedRetry::Existing(snapshot));
        }
        let preparation = (|| -> Result<PreparedRetry, ProductRunServiceError> {
            if record_view.resume_decode_pending() || record_view.handoff_recovery_pending {
                return Err(ProductRunServiceError::Unavailable);
            }
            let retry_handoff_coverage = if record_view.rejected_finding_update.is_some() {
                let governed = self
                    .with_control_conversation(
                        record_view.interaction.workbench.conversation(),
                        |store| store.capture_execution(&record_view.interaction.workbench),
                    )?
                    .inputs()
                    .conversation()
                    .to_owned();
                let workbench_root = self
                    .inner
                    .directory
                    .parent()
                    .ok_or_else(|| {
                        ProductRunServiceError::internal(
                            "validate retry handoff coverage",
                            "the product-run directory has no workbench parent",
                        )
                    })?
                    .join("workbench-v1");
                Some(super::persistence::validate_retry_handoff_coverage(
                    &workbench_root,
                    &governed,
                    &record_view,
                )?)
            } else {
                None
            };
            let reset_finding_catalog = (!recovering_continuation)
                .then(|| super::persistence::FindingBodyStore::catalog(""))
                .transpose()?;
            self.ensure_run_admission()?;
            if self.run_owner_active(run_id)? {
                return Err(ProductRunServiceError::InvalidState);
            }
            let records = self
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)?
                .clone();
            if workspace_has_active_run(&records, workspace_id, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            // Retain the exact attempt owner before the new attempt becomes canonical. Once the
            // durable retry mutation is visible, cancellation and shutdown must always have an
            // owner to reconcile; every error below drops this unpublished guard.
            let prepared_owner = self.register_run_cancellation(
                run_id,
                &cancelled,
                &control_cancellation,
                &provider_cancellation,
                &record_view.interaction.workbench,
                owned_continuation.map(|source| source.operation),
            )?;
            let ((request, finding_state, resume, snapshot), ticket) = self.mutate_run(
                run_id,
                Some(&record_view.cancelled),
                RunMutationKind::RetryAdmission,
                lifecycle_input(b"retry-admission"),
                MutationDisposition::DurabilityRequired,
                |record| {
                let retry_authority_changed = retry_handoff_coverage
                    .as_ref()
                    .map(|proof| proof.binds(record))
                    .transpose()?
                    .is_some_and(|bound| !bound);
                if !Arc::ptr_eq(&record.cancelled, &record_view.cancelled)
                    || record.record_revision != record_view.record_revision
                    || record.attempt_sequence != record_view.attempt_sequence
                    || record.handoff_sequence != record_view.handoff_sequence
                    || record.handoff_recovery_pending
                    || record.interaction.workbench != record_view.interaction.workbench
                    || record.interaction.mode != record_view.interaction.mode
                    || record.interaction.models != record_view.interaction.models
                    || record.continuation_admissions != record_view.continuation_admissions
                    || record.continuation_sources != record_view.continuation_sources
                    || record.rejected_finding_update != record_view.rejected_finding_update
                    || record.finding_catalog.head_digest
                        != record_view.finding_catalog.head_digest
                    || retry_authority_changed
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                let next_attempt_sequence =
                    record.attempt_sequence.checked_add(1).ok_or_else(|| {
                        ProductRunServiceError::internal(
                            "prepare durable product-run retry",
                            "attempt sequence overflow",
                        )
                    })?;
                if record.rejected_finding_update.as_ref().is_some_and(|rejected| {
                    record.handoff_sequence == 0
                        || rejected.accepted_head != record.finding_catalog.head_digest
                }) {
                    return Err(ProductRunServiceError::InvalidState);
                }
                if let Some(admission) = admission {
                    match admission {
                        RetryAdmission::Goal(operation) => {
                            record.attempt_admission = Some(operation.id());
                        }
                        RetryAdmission::Continuation { operation, .. } => {
                            let source = continuation_source
                                .ok_or(ProductRunServiceError::InvalidState)?;
                            if !record.continuation_admissions.contains(&operation.id()) {
                                record.continuation_admissions.push(operation.id());
                            }
                            match record
                                .continuation_sources
                                .iter()
                                .find(|retained| retained.operation == operation.id())
                            {
                                Some(retained) if *retained == source => {}
                                Some(_) => return Err(ProductRunServiceError::InvalidState),
                                None => record.continuation_sources.push(source),
                            }
                        }
                    }
                }
                // The immutable rejection handoff remains retained, but its diagnostic belongs to
                // the covered attempt and must not govern the newly admitted mutable projection.
                record.rejected_finding_update = None;
                if explicit_continuation {
                    record.interaction.mode = attempt_interaction.mode;
                    record.interaction.models.clone_from(&attempt_interaction.models);
                    if !recovering_continuation {
                        if let Err(error) = record.interaction.append(
                            peritus_app_protocol::ProductActivityKind::Status,
                            "Continuation launch admitted from durable inputs.",
                            "",
                        ) {
                            return Err(error);
                        }
                        record.resume = None;
                        record.finding_state.clear();
                        record.finding_catalog = reset_finding_catalog
                            .clone()
                            .ok_or(ProductRunServiceError::Unavailable)?;
                    }
                }
                record.cancelled = Arc::clone(&cancelled);
                record.control_cancellation = control_cancellation.clone();
                record.user_cancelled = cancelled.load(Ordering::Acquire);
                record.provider_cancellation = provider_cancellation.clone();
                record.attempt_sequence = next_attempt_sequence;
                record.snapshot = match super::snapshot::retry_snapshot(
                    &record.request,
                    record.resume.as_ref(),
                ) {
                    Ok(snapshot) => snapshot,
                        Err(error) => return Err(error),
                };
                if !recovering_continuation {
                    record.progress.begin_attempt();
                }
                record.settlement = None;
                record.interruption_cause.clear();
                    Ok((
                    record.request.clone(),
                    record.finding_state.clone(),
                    record.resume.clone(),
                    record.snapshot.clone(),
                    ))
                },
            )?;
            self.await_run_durable(ticket)?;
            if cancelled.load(Ordering::Acquire) {
                let snapshot = self.publish_retry_migration_cancellation(run_id, &cancelled)?;
                return Ok(PreparedRetry::Existing(snapshot));
            }
            if cancelled.load(Ordering::Acquire) || self.user_cancellation_requested(run_id) {
                drop(prepared_owner);
                let snapshot = self.publish_retry_migration_cancellation(run_id, &cancelled)?;
                return Ok(PreparedRetry::Existing(snapshot));
            }
            Ok(PreparedRetry::Launch(PreparedRunLaunch {
                request,
                workspace_root,
                providers,
                cancelled: Arc::clone(&cancelled),
                provider_cancellation: provider_cancellation.clone(),
                finding_state,
                resume,
                snapshot,
                owner: prepared_owner,
            }))
        })();
        preparation
    }

    fn publish_retry_migration_cancellation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let generation = self.pending_user_cancellation_generation(run, attempt);
        let cancellation_kind = if self.inner.control_shutdown.is_cancelled() {
            RunMutationKind::ShutdownSettlement
        } else {
            RunMutationKind::Cancellation
        };
        let ((mut handoff, prior_sequence, attempt_sequence, snapshot), ticket) = self.mutate_run(
            run,
            Some(attempt),
            cancellation_kind,
            lifecycle_input(b"retry-preparation-cancellation"),
            MutationDisposition::DurabilityRequired,
            |record| {
                let prior_sequence = record.handoff_sequence;
                let attempt_sequence = record.attempt_sequence;
                let mut handoff = record.clone();
                handoff.handoff_sequence =
                    prior_sequence.checked_add(1).ok_or_else(|| {
                        ProductRunServiceError::internal(
                            "publish retry-preparation cancellation handoff",
                            "handoff sequence overflow",
                        )
                    })?;
                handoff.user_cancelled = true;
                handoff.snapshot = replace_snapshot(
                    &handoff.snapshot,
                    ProductRunPhase::Cancelled,
                    "Run cancelled",
                    "Cancelled while durable retry preparation completed",
                )?;
                handoff.handoff_recovery_pending = true;
                record.user_cancelled = true;
                record.snapshot = handoff.snapshot.clone();
                record.handoff_recovery_pending = true;
                Ok((
                    handoff,
                    prior_sequence,
                    attempt_sequence,
                    record.snapshot.clone(),
                ))
            },
        )?;
        self.await_run_durable(ticket)?;
        super::persistence::write_handoff_retrying(
            &self.inner.directory,
            &handoff,
            super::persistence::HandoffKind::Terminal,
            &self.inner.control_shutdown,
        )?;
        handoff.handoff_recovery_pending = false;
        let (_, ticket) = self.mutate_run(
            run,
            Some(attempt),
            if self.inner.control_shutdown.is_cancelled() {
                RunMutationKind::ShutdownSettlement
            } else {
                RunMutationKind::Recovery
            },
            lifecycle_input(b"install-retry-preparation-cancellation"),
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.attempt_sequence != attempt_sequence
                    || record.handoff_sequence != prior_sequence
                    || !record.user_cancelled
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                handoff.interaction = record.interaction.clone();
                handoff.preview = record.preview.clone();
                install_handoff_record(record, handoff);
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)?;
        if let Some(generation) = generation {
            self.acknowledge_user_cancellation(run, attempt, generation)?;
        }
        Ok(snapshot)
    }
}

fn install_handoff_record(record: &mut super::RunRecord, replacement: super::RunRecord) {
    let record_revision = record.record_revision;
    let record_lineage_root = record.record_lineage_root;
    let durable_record_revision = record.durable_record_revision;
    let durable_lineage_root = record.durable_lineage_root;
    let durable_canonical_digest = record.durable_canonical_digest;
    let settlement_obligation = record.settlement_obligation.clone();
    *record = replacement;
    record.record_revision = record_revision;
    record.record_lineage_root = record_lineage_root;
    record.durable_record_revision = durable_record_revision;
    record.durable_lineage_root = durable_lineage_root;
    record.durable_canonical_digest = durable_canonical_digest;
    record.settlement_obligation = settlement_obligation;
}

fn lifecycle_input(value: &[u8]) -> Sha256Digest {
    peritus_codec::sha256(value)
}
