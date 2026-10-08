//! Blocking ownership boundaries for run admission, retry launch, and cancellation.

mod queued;

use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use peritus_app_protocol::ProductRunSnapshot;
use peritus_product_runner::{ProductRunResume, RoleProviders};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;

use super::{
    PreviewAggregate, ProductRunRequest, ProductRunService, ProductRunServiceError, RunCancellation,
    RunProgress, RunRecord, deliverable, interaction,
};
use super::snapshot::{initial_snapshot, workspace_has_active_run};

pub(super) struct PreparedRunLaunch {
    pub(super) request: ProductRunRequest,
    pub(super) workspace_root: PathBuf,
    pub(super) providers: RoleProviders,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) provider_cancellation: CancellationToken,
    pub(super) finding_state: String,
    pub(super) resume: Option<ProductRunResume>,
    pub(super) snapshot: ProductRunSnapshot,
    pub(super) owner: PreparedRunOwner,
}

/// Retires an exact attempt that finished blocking preparation but was never published as a task.
///
/// `spawn_blocking` continues after its awaiting future is dropped. Keeping this guard in the
/// returned value makes the detached result retire itself when Tokio discards it. Drop performs
/// no physical I/O: an accepted cancellation has a synchronous retainer, while an unclaimed
/// owner-boundary signal is retracted so the original durable record remains recoverable.
pub(super) struct PreparedRunOwner {
    service: ProductRunService,
    run: RunId,
    attempt: Arc<AtomicBool>,
    published: bool,
}

#[cfg(test)]
pub(super) struct PreparedOwnerBarrier {
    reached: tokio::sync::Notify,
    released: std::sync::Mutex<bool>,
    ready: std::sync::Condvar,
}

#[cfg(test)]
static PREPARED_OWNER_BARRIERS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::BTreeMap<[u8; 16], Arc<PreparedOwnerBarrier>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));

#[cfg(test)]
impl PreparedOwnerBarrier {
    pub(super) async fn reached(&self) {
        self.reached.notified().await;
    }

    pub(super) fn release(&self) {
        if let Ok(mut released) = self.released.lock() {
            *released = true;
            self.ready.notify_one();
        }
    }
}

#[cfg(test)]
pub(super) fn inject_prepared_owner_barrier(run: RunId) -> Arc<PreparedOwnerBarrier> {
    let barrier = Arc::new(PreparedOwnerBarrier {
        reached: tokio::sync::Notify::new(),
        released: std::sync::Mutex::new(false),
        ready: std::sync::Condvar::new(),
    });
    PREPARED_OWNER_BARRIERS
        .lock()
        .expect("prepared owner barrier lock")
        .insert(run.into_bytes(), Arc::clone(&barrier));
    barrier
}

#[cfg(test)]
fn pause_prepared_owner(run: RunId) {
    let barrier = PREPARED_OWNER_BARRIERS
        .lock()
        .ok()
        .and_then(|mut barriers| barriers.remove(&run.into_bytes()));
    let Some(barrier) = barrier else { return };
    barrier.reached.notify_one();
    let Ok(mut released) = barrier.released.lock() else { return };
    while !*released {
        let Ok(next) = barrier.ready.wait(released) else { return };
        released = next;
    }
}

impl PreparedRunOwner {
    fn published(mut self) {
        self.published = true;
    }
}

impl Drop for PreparedRunOwner {
    fn drop(&mut self) {
        if !self.published {
            self.service.retire_unpublished_run_cancellation(self.run, &self.attempt);
        }
    }
}

pub(super) enum PreparedRetry {
    Existing(ProductRunSnapshot),
    Launch(PreparedRunLaunch),
}

impl ProductRunService {
    pub(super) async fn start_configured(
        &self,
        request: ProductRunRequest,
        interaction: interaction::InteractionOptions,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let service = self.clone();
        let prepared = Self::await_blocking_owner("prepare durable product-run admission", move || {
            service.prepare_start_configured(request, interaction)
        })
        .await?;
        let snapshot = prepared.snapshot.clone();
        self.launch_prepared(prepared).await?;
        Ok(snapshot)
    }

    fn prepare_start_configured(
        &self,
        request: ProductRunRequest,
        mut interaction: interaction::InteractionOptions,
    ) -> Result<PreparedRunLaunch, ProductRunServiceError> {
        self.validate_workspace_mode(request.workspace_id(), interaction.mode)?;
        let providers = self.resolve_selected_providers(request.providers(), &interaction)?;
        let workspace_root = self
            .inner
            .workspaces
            .get(&request.workspace_id())
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        let snapshot = initial_snapshot(&request)?;
        self.inner
            .product_artifacts
            .publish_text(request.run_id(), request.execution_task())?;
        self.inner
            .product_artifacts
            .publish_snapshot(request.run_id(), &snapshot)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let control_cancellation = peritus_journal::JournalCancellation::new();
        let provider_cancellation = CancellationToken::new();
        let finding_catalog = super::persistence::FindingBodyStore::catalog("")?;
        self.append_control_inputs(&mut interaction)?;
        self.ensure_run_admission()?;
        if self.run_owner_active(request.run_id())? {
            return Err(ProductRunServiceError::InvalidState);
        }
        let records_snapshot = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .clone();
        let mut resume_staged = false;
        let mut staged_accepted = false;
        let mut recover_staged = false;
        if let Some(staged) = records_snapshot.get(&request.run_id()) {
            let proposed = &interaction.workbench;
            let existing = &staged.interaction.workbench;
            let resolved = if proposed == existing {
                self.with_control_authorities(
                    crate::product_control::AuthoritySet::new([
                        crate::product_control::AuthorityKey::Conversation(
                            proposed.conversation(),
                        ),
                        crate::product_control::AuthorityKey::Run(request.run_id()),
                    ]),
                    |store| store.resolve(proposed),
                )?
            } else {
                None
            };
            let initial_shape = staged.attempt_sequence == 1
                && staged.attempt_admission.is_none()
                && staged.continuation_admissions.is_empty()
                && staged.continuation_sources.is_empty()
                && staged.settlement_obligation.is_none()
                && staged.checkpoint.is_none()
                && staged.settlement.is_none()
                && staged.resume.is_none()
                && staged.opaque_resume.is_none();
            resume_staged = proposed == existing
                && staged.snapshot.phase() == peritus_app_protocol::ProductRunPhase::Queued
                && initial_shape;
            staged_accepted = resume_staged && resolved.is_some();
            recover_staged = proposed == existing
                && staged.snapshot.phase()
                    == peritus_app_protocol::ProductRunPhase::RecoveryRequired
                && initial_shape;
            if !resume_staged && !recover_staged {
                return Err(ProductRunServiceError::Duplicate);
            }
        }
        if workspace_has_active_run(
            &records_snapshot,
            request.workspace_id(),
            (resume_staged || recover_staged).then_some(request.run_id()),
        ) {
            return Err(ProductRunServiceError::InvalidState);
        }
        deliverable::discard::workspace_available(
            &self.inner.directory,
            &records_snapshot,
            request.workspace_id(),
        )?;
        if resume_staged {
            return self.resume_staged_start(
                records_snapshot
                    .get(&request.run_id())
                    .ok_or(ProductRunServiceError::NotFound)?,
                request,
                workspace_root,
                providers,
                !staged_accepted,
            );
        }
        let catalog_sequence = match records_snapshot.get(&request.run_id()) {
            Some(record) => record.progress.catalog_sequence,
            None => self
                .inner
                .model_catalogs
                .runs_mut()?
                .allocate_sequence(&self.inner.directory)?,
        };
        if !recover_staged {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            if workspace_has_active_run(&records, request.workspace_id(), Some(request.run_id())) {
                return Err(ProductRunServiceError::InvalidState);
            }
            if catalog_sequence == 0 {
                return Err(ProductRunServiceError::InvalidState);
            }
            let mut progress = RunProgress::default();
            progress.catalog_sequence = catalog_sequence;
            records.insert(
                request.run_id(),
                RunRecord {
                    attempt_sequence: 1,
                    handoff_sequence: 0,
                    record_revision: 0,
                    record_lineage_root: peritus_types::Sha256Digest::new([0; 32]),
                    durable_record_revision: 0,
                    durable_lineage_root: peritus_types::Sha256Digest::new([0; 32]),
                    durable_canonical_digest: peritus_types::Sha256Digest::new([0; 32]),
                    settlement_obligation: None,
                    review_artifact_migration_version:
                        super::persistence::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION,
                    handoff_recovery_pending: false,
                    rejected_finding_update: None,
                    interaction: interaction.clone(),
                    attempt_admission: None,
                    continuation_admissions: Vec::new(),
                    continuation_sources: Vec::new(),
                    request: request.clone(),
                    snapshot: snapshot.clone(),
                    cancelled: Arc::clone(&cancelled),
                    control_cancellation: control_cancellation.clone(),
                    user_cancelled: false,
                    provider_cancellation: provider_cancellation.clone(),
                    finding_state: String::new(),
                    finding_catalog: finding_catalog.clone(),
                    progress,
                    checkpoint: None,
                    settlement: None,
                    resume: None,
                    opaque_resume: None,
                    remaining_work: Vec::new(),
                    interruption_cause: String::new(),
                    candidate_actionable: false,
                    task_baseline_required: !self
                        .inner
                        .folders
                        .contains_key(&request.workspace_id()),
                    task_baseline: None,
                    preview: PreviewAggregate::default(),
                },
            );
        }
        let proposed = interaction.workbench.clone();
        let expected_attempt = if recover_staged {
            records_snapshot
                .get(&request.run_id())
                .map(|record| Arc::clone(&record.cancelled))
                .ok_or(ProductRunServiceError::NotFound)?
        } else {
            Arc::clone(&cancelled)
        };
        let admission_input = peritus_codec::sha256(proposed.id().as_bytes());
        let (_, ticket) = self.mutate_run(
            request.run_id(),
            Some(&expected_attempt),
            super::publication::RunMutationKind::InitialAdmission,
            admission_input,
            super::publication::MutationDisposition::DurabilityRequired,
            |record| {
                if recover_staged {
                    let catalog_sequence = record.progress.catalog_sequence;
                    let record_revision = record.record_revision;
                    let record_lineage_root = record.record_lineage_root;
                    let durable_record_revision = record.durable_record_revision;
                    let durable_lineage_root = record.durable_lineage_root;
                    let durable_canonical_digest = record.durable_canonical_digest;
                    let mut progress = RunProgress::default();
                    progress.catalog_sequence = catalog_sequence;
                    *record = RunRecord {
                        attempt_sequence: 1,
                        handoff_sequence: 0,
                        record_revision,
                        record_lineage_root,
                        durable_record_revision,
                        durable_lineage_root,
                        durable_canonical_digest,
                        settlement_obligation: None,
                        review_artifact_migration_version:
                            super::persistence::CURRENT_REVIEW_ARTIFACT_MIGRATION_VERSION,
                        handoff_recovery_pending: false,
                        rejected_finding_update: None,
                        interaction,
                        attempt_admission: None,
                        continuation_admissions: Vec::new(),
                        continuation_sources: Vec::new(),
                        request: request.clone(),
                        snapshot: snapshot.clone(),
                        cancelled: Arc::clone(&cancelled),
                        control_cancellation: control_cancellation.clone(),
                        user_cancelled: false,
                        provider_cancellation: provider_cancellation.clone(),
                        finding_state: String::new(),
                        finding_catalog: finding_catalog.clone(),
                        progress,
                        checkpoint: None,
                        settlement: None,
                        resume: None,
                        opaque_resume: None,
                        remaining_work: Vec::new(),
                        interruption_cause: String::new(),
                        candidate_actionable: false,
                        task_baseline_required: !self
                            .inner
                            .folders
                            .contains_key(&request.workspace_id()),
                        task_baseline: None,
                        preview: PreviewAggregate::default(),
                    };
                }
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)?;
        let current = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&request.run_id())
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        self.inner.model_catalogs.runs_mut()?.replace(&current)?;
        self.with_control_authorities(
            crate::product_control::AuthoritySet::new([
                crate::product_control::AuthorityKey::Conversation(proposed.conversation()),
                crate::product_control::AuthorityKey::Run(request.run_id()),
            ]),
            |store| store.accept(&proposed),
        )?;
        let prepared_owner = self.register_run_cancellation(
            request.run_id(),
            &cancelled,
            &current.control_cancellation,
            &provider_cancellation,
            &current.interaction.workbench,
            None,
        )?;
        Ok(PreparedRunLaunch {
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            finding_state: String::new(),
            resume: None,
            snapshot,
            owner: prepared_owner,
        })
    }

    fn resume_staged_start(
        &self,
        staged: &RunRecord,
        request: ProductRunRequest,
        workspace_root: PathBuf,
        providers: RoleProviders,
        accept_control: bool,
    ) -> Result<PreparedRunLaunch, ProductRunServiceError> {
        if staged.request.run_id() != request.run_id()
            || staged.request.workspace_id() != request.workspace_id()
            || staged.request.providers() != request.providers()
            || staged.request.execution_task() != request.execution_task()
            || staged.request.display_task() != request.display_task()
            || staged.attempt_sequence != 1
            || staged.settlement_obligation.is_some()
        {
            return Err(ProductRunServiceError::Duplicate);
        }
        let attempt = Arc::clone(&staged.cancelled);
        let operation = staged.interaction.workbench.id();
        let (_, ticket) = self.mutate_run(
            request.run_id(),
            Some(&attempt),
            super::publication::RunMutationKind::InitialAdmission,
            peritus_codec::sha256(operation.as_bytes()),
            super::publication::MutationDisposition::DurabilityRequired,
            |record| {
                if record.snapshot.phase() != peritus_app_protocol::ProductRunPhase::Queued
                    || record.interaction.workbench.id() != operation
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)?;
        let current = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&request.run_id())
            .cloned()
            .ok_or(ProductRunServiceError::NotFound)?;
        self.inner.model_catalogs.runs_mut()?.replace(&current)?;
        if accept_control {
            let proposed = current.interaction.workbench.clone();
            self.with_control_authorities(
                crate::product_control::AuthoritySet::new([
                    crate::product_control::AuthorityKey::Conversation(proposed.conversation()),
                    crate::product_control::AuthorityKey::Run(request.run_id()),
                ]),
                |store| store.accept(&proposed),
            )?;
        }
        let owner = self.register_run_cancellation(
            request.run_id(),
            &current.cancelled,
            &current.control_cancellation,
            &current.provider_cancellation,
            &current.interaction.workbench,
            None,
        )?;
        Ok(PreparedRunLaunch {
            request: current.request,
            workspace_root,
            providers,
            cancelled: current.cancelled,
            provider_cancellation: current.provider_cancellation,
            finding_state: current.finding_state,
            resume: current.resume,
            snapshot: current.snapshot,
            owner,
        })
    }

    pub(super) async fn launch_prepared(
        &self,
        prepared: PreparedRunLaunch,
    ) -> Result<(), ProductRunServiceError> {
        let PreparedRunLaunch {
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            finding_state,
            resume,
            snapshot: _,
            owner,
        } = prepared;
        let run_id = request.run_id();
        let seal_attempt = Arc::clone(&cancelled);
        let launched = self.spawn(
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            finding_state,
            resume,
        )
        .await;
        match launched {
            Ok(()) => {
                owner.published();
                Ok(())
            }
            Err(error) => {
                let service = self.clone();
                let sealed = Self::await_blocking_owner(
                    "seal unlaunched product-run owner",
                    move || service.seal_run_owner(run_id, &seal_attempt),
                )
                .await;
                drop(owner);
                sealed?;
                Err(error)
            }
        }
    }

    pub(super) fn discard_prepared_run(
        &self,
        prepared: PreparedRunLaunch,
    ) -> Result<(), ProductRunServiceError> {
        let PreparedRunLaunch {
            request,
            cancelled,
            owner,
            ..
        } = prepared;
        let run = request.run_id();
        let input = peritus_codec::sha256(
            b"peritus-product-run-discard-unpublished-preparation-v1",
        );
        let discarded = (|| {
            let (_, ticket) = self.mutate_run(
                run,
                Some(&cancelled),
                super::RunMutationKind::Recovery,
                input,
                super::MutationDisposition::DurabilityRequired,
                |record| {
                    record.candidate_actionable = false;
                    record.interruption_cause =
                        "Prepared launch stopped before its durable admission completed".to_owned();
                    record.snapshot = super::replace_snapshot(
                        &record.snapshot,
                        peritus_app_protocol::ProductRunPhase::RecoveryRequired,
                        "Prepared launch requires recovery",
                        &record.interruption_cause,
                    )?;
                    Ok(())
                },
            )?;
            self.await_run_durable(ticket)
        })();
        let sealed = self.seal_run_owner(run, &cancelled);
        drop(owner);
        discarded.and(sealed)
    }

    pub(super) fn ensure_run_admission(&self) -> Result<(), ProductRunServiceError> {
        if self.inner.control_shutdown.is_cancelled() {
            Err(ProductRunServiceError::Unavailable)
        } else {
            Ok(())
        }
    }

    pub(super) async fn await_blocking_owner<T>(
        operation: &'static str,
        task: impl FnOnce() -> Result<T, ProductRunServiceError> + Send + 'static,
    ) -> Result<T, ProductRunServiceError>
    where
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(task).await.map_err(|error| {
            ProductRunServiceError::internal(operation, error.to_string())
        })?
    }

    pub(super) async fn await_blocking_future<T, F, Fut>(
        operation: &'static str,
        task: F,
    ) -> Result<T, ProductRunServiceError>
    where
        T: Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
    {
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || runtime.block_on(task()))
            .await
            .map_err(|error| ProductRunServiceError::internal(operation, error.to_string()))
    }

    pub(super) fn register_run_cancellation(
        &self,
        run: RunId,
        cancelled: &Arc<AtomicBool>,
        control: &peritus_journal::JournalCancellation,
        provider: &CancellationToken,
        binding: &peritus_product_runner::control::ControlOperation,
        continuation: Option<peritus_product_runner::control::OperationId>,
    ) -> Result<PreparedRunOwner, ProductRunServiceError> {
        let reconciliation = Arc::new(self.register_control_reconciliation(
            crate::product_control::AuthoritySet::new([
                crate::product_control::AuthorityKey::Conversation(binding.conversation()),
                crate::product_control::AuthorityKey::Run(run),
            ]),
        )?);
        let mut cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        if cancellations
            .get(&run)
            .is_some_and(|owner| owner.active.load(Ordering::Acquire))
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        let cancellation = RunCancellation {
            actor: *binding.actor_bytes(),
            continuation,
            attempt_cancelled: Arc::clone(cancelled),
            control: control.clone(),
            provider: provider.clone(),
            reconciliation: Some(reconciliation),
            user_requested: AtomicBool::new(false),
            requested_generation: 0,
            acknowledged_generation: 0,
            cancellation_retainer: false,
            owner_dropped: false,
            active: AtomicBool::new(true),
            launched: AtomicBool::new(false),
            ungoverned: matches!(
                binding.intent(),
                peritus_product_runner::control::ControlIntent::StartExecution { .. }
            ),
        };
        if self.inner.control_shutdown.is_cancelled() {
            cancellation
                .attempt_cancelled
                .store(true, std::sync::atomic::Ordering::Release);
            let _ = cancellation.provider.cancel();
            cancellation.control.cancel();
        }
        cancellations.insert(run, cancellation);
        let owner = PreparedRunOwner {
            service: self.clone(),
            run,
            attempt: Arc::clone(cancelled),
            published: false,
        };
        drop(cancellations);
        #[cfg(test)]
        pause_prepared_owner(run);
        Ok(owner)
    }

    pub(super) fn retained_control_reconciliation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<Arc<crate::product_control::ControlReconciliation>, ProductRunServiceError> {
        let cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let cancellation = cancellations.get(&run).ok_or(ProductRunServiceError::InvalidState)?;
        if !Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
            || !cancellation.active.load(Ordering::Acquire)
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        cancellation
            .reconciliation
            .as_ref()
            .cloned()
            .ok_or(ProductRunServiceError::InvalidState)
    }

    pub(super) fn continuation_owner_live(
        &self,
        run: RunId,
        operation: peritus_product_runner::control::OperationId,
    ) -> Result<bool, ProductRunServiceError> {
        let cancellations = self
            .inner
            .run_cancellations
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        Ok(cancellations.get(&run).is_some_and(|owner| {
            owner.continuation == Some(operation)
                && owner.active.load(Ordering::Acquire)
                && owner.launched.load(Ordering::Acquire)
        }))
    }

    pub(super) fn run_owner_active(&self, run: RunId) -> Result<bool, ProductRunServiceError> {
        let cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        Ok(cancellations
            .get(&run)
            .is_some_and(|owner| owner.active.load(Ordering::Acquire)))
    }

    pub(super) fn continuation_owner_retained(
        &self,
        run: RunId,
        operation: peritus_product_runner::control::OperationId,
    ) -> Result<bool, ProductRunServiceError> {
        let cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        Ok(cancellations.get(&run).is_some_and(|owner| {
            owner.continuation == Some(operation) && owner.active.load(Ordering::Acquire)
        }))
    }

    pub(super) fn signal_user_cancellation(&self, run: RunId) -> bool {
        let Ok(mut cancellations) = self.inner.run_cancellations.lock() else { return false };
        let Some(cancellation) = cancellations.get_mut(&run) else { return false };
        if cancellation.active.load(Ordering::Acquire) {
            Self::request_user_cancellation(cancellation);
        }
        true
    }

    pub(super) fn signal_authenticated_user_cancellation(
        &self,
        actor: peritus_types::ActorId,
        run: RunId,
    ) -> Result<bool, ProductRunServiceError> {
        let mut cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let Some(cancellation) = cancellations.get_mut(&run) else { return Ok(false) };
        if cancellation.actor != *actor.as_bytes() {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::ScopeMismatch,
            ));
        }
        if !cancellation.ungoverned || !cancellation.active.load(Ordering::Acquire) {
            return Ok(false);
        }
        Self::request_user_cancellation(cancellation);
        Ok(true)
    }

    fn request_user_cancellation(cancellation: &mut super::RunCancellation) -> u64 {
        if !cancellation.user_requested.swap(true, Ordering::AcqRel) {
            cancellation.requested_generation =
                cancellation.acknowledged_generation.saturating_add(1);
        }
        cancellation.attempt_cancelled.store(true, Ordering::Release);
        let _ = cancellation.provider.cancel();
        cancellation.control.cancel();
        cancellation.requested_generation
    }

    pub(super) fn request_user_cancellation_for_attempt(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<Option<(u64, bool)>, ProductRunServiceError> {
        let mut cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let Some(cancellation) = cancellations.get_mut(&run) else { return Ok(None) };
        if !Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
            || !cancellation.active.load(Ordering::Acquire)
        {
            return Ok(None);
        }
        let generation = Self::request_user_cancellation(cancellation);
        // The synchronous caller now owns canonical retention or an existing joined publisher.
        // Prepared-owner Drop may retract only an unclaimed pre-acceptance signal.
        cancellation.cancellation_retainer = true;
        Ok(Some((
            generation,
            cancellation.launched.load(Ordering::Acquire),
        )))
    }

    pub(super) fn pending_user_cancellation_generation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Option<u64> {
        self.inner.run_cancellations.lock().ok().and_then(|cancellations| {
            cancellations.get(&run).and_then(|cancellation| {
                (Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
                    && cancellation.active.load(Ordering::Acquire)
                    && cancellation.requested_generation
                        > cancellation.acknowledged_generation)
                    .then_some(cancellation.requested_generation)
            })
        })
    }

    pub(super) fn acknowledge_user_cancellation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
        generation: u64,
    ) -> Result<(), ProductRunServiceError> {
        let terminal_obligation_resolved =
            self.terminal_obligation_resolved(run, attempt)?;
        let mut cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let cancellation = cancellations.get_mut(&run).ok_or(ProductRunServiceError::InvalidState)?;
        if !Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
            || generation == 0
            || generation > cancellation.requested_generation
        {
            return Err(ProductRunServiceError::InvalidState);
        }
        if !cancellation.active.load(Ordering::Acquire) {
            return if cancellation.owner_dropped
                && generation <= cancellation.acknowledged_generation
            {
                Ok(())
            } else {
                Err(ProductRunServiceError::InvalidState)
            };
        }
        cancellation.acknowledged_generation =
            cancellation.acknowledged_generation.max(generation);
        if cancellation.owner_dropped
            && cancellation.requested_generation == cancellation.acknowledged_generation
            && (!cancellation.launched.load(Ordering::Acquire)
                || terminal_obligation_resolved)
        {
            cancellation.active.store(false, Ordering::Release);
            cancellation.reconciliation = None;
        }
        Ok(())
    }

    pub(super) fn dismiss_user_cancellation_for_attempt(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) {
        let Ok(mut cancellations) = self.inner.run_cancellations.lock() else { return };
        let Some(cancellation) = cancellations.get_mut(&run) else { return };
        if Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
            && cancellation.active.load(Ordering::Acquire)
        {
            cancellation.acknowledged_generation = cancellation.requested_generation;
            cancellation.user_requested.store(false, Ordering::Release);
            cancellation.cancellation_retainer = false;
            if cancellation.owner_dropped && !cancellation.launched.load(Ordering::Acquire) {
                cancellation.active.store(false, Ordering::Release);
                cancellation.reconciliation = None;
            }
        }
    }

    /// Atomically retires an exact owner or returns the cancellation generation it must publish.
    ///
    /// Callers hold the records registry before entering this short owner boundary. No filesystem
    /// operation is permitted while the cancellation registry is held.
    pub(super) fn seal_run_cancellation(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<Option<u64>, ProductRunServiceError> {
        let terminal_obligation_resolved =
            self.terminal_obligation_resolved(run, attempt)?;
        let mut cancellations =
            self.inner.run_cancellations.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let cancellation =
            cancellations.get_mut(&run).ok_or(ProductRunServiceError::InvalidState)?;
        if !Arc::ptr_eq(&cancellation.attempt_cancelled, attempt) {
            return Err(ProductRunServiceError::InvalidState);
        }
        if !cancellation.active.load(Ordering::Acquire) {
            return if cancellation.requested_generation
                == cancellation.acknowledged_generation
            {
                Ok(None)
            } else {
                Err(ProductRunServiceError::InvalidState)
            };
        }
        if cancellation.requested_generation == cancellation.acknowledged_generation {
            if cancellation.launched.load(Ordering::Acquire)
                && !terminal_obligation_resolved
            {
                return Err(ProductRunServiceError::InvalidState);
            }
            cancellation.active.store(false, Ordering::Release);
            cancellation.reconciliation = None;
            Ok(None)
        } else {
            Ok(Some(cancellation.requested_generation))
        }
    }

    pub(super) fn retire_run_cancellation(&self, run: RunId, attempt: &Arc<AtomicBool>) {
        let terminal_obligation_resolved = self
            .terminal_obligation_resolved(run, attempt)
            .unwrap_or(false);
        let Ok(mut cancellations) = self.inner.run_cancellations.lock() else { return };
        let Some(cancellation) = cancellations.get_mut(&run) else { return };
        if Arc::ptr_eq(&cancellation.attempt_cancelled, attempt) {
            cancellation.owner_dropped = true;
            if cancellation.requested_generation == cancellation.acknowledged_generation {
                if !cancellation.launched.load(Ordering::Acquire)
                    || terminal_obligation_resolved
                {
                    cancellation.active.store(false, Ordering::Release);
                    cancellation.reconciliation = None;
                }
            }
        }
    }

    fn terminal_obligation_resolved(
        &self,
        run: RunId,
        attempt: &Arc<AtomicBool>,
    ) -> Result<bool, ProductRunServiceError> {
        let records =
            self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&run).ok_or(ProductRunServiceError::NotFound)?;
        Ok(Arc::ptr_eq(&record.cancelled, attempt)
            && record.snapshot.phase().terminal()
            && record
                .settlement_obligation
                .as_ref()
                .is_some_and(super::persistence::SettlementObligation::resolved))
    }

    fn retire_unpublished_run_cancellation(&self, run: RunId, attempt: &Arc<AtomicBool>) {
        let Ok(mut cancellations) = self.inner.run_cancellations.lock() else { return };
        let Some(cancellation) = cancellations.get_mut(&run) else { return };
        if Arc::ptr_eq(&cancellation.attempt_cancelled, attempt)
            && !cancellation.launched.load(Ordering::Acquire)
        {
            cancellation.owner_dropped = true;
            if cancellation.requested_generation > cancellation.acknowledged_generation
                && !cancellation.cancellation_retainer
            {
                // A signal only becomes an accepted cancellation when `cancel` retains its
                // canonical projection. Dropping the unpublished preparation abandons an
                // unaccepted signal so the original durable record remains recoverable.
                cancellation.requested_generation = cancellation.acknowledged_generation;
                cancellation.user_requested.store(false, Ordering::Release);
                cancellation.attempt_cancelled.store(false, Ordering::Release);
            }
            if cancellation.requested_generation == cancellation.acknowledged_generation {
                cancellation.active.store(false, Ordering::Release);
                cancellation.reconciliation = None;
            }
        }
    }

    pub(super) fn user_cancellation_requested(&self, run: RunId) -> bool {
        self.inner
            .run_cancellations
            .lock()
            .ok()
            .and_then(|cancellations| {
                cancellations
                    .get(&run)
                    .map(|cancellation| cancellation.user_requested.load(Ordering::Acquire))
            })
            .unwrap_or(false)
    }
}
