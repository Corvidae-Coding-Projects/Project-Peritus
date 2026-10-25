//! Real E3 campaign creation, full-record recovery, analysis, and claimed publication.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, FinalizedArtifact, MediaType, WriteRequest,
};
use peritus_eval::{
    EvaluationArm, EvaluationCampaignId, EvaluationCommand, EvaluationCommandKind,
    EvaluationDirectiveClaim, EvaluationEventKind, EvaluationExecutionMode, EvaluationLimits,
    EvaluationPhase, EvaluationPlan, EvaluationReport, EvaluationState, FrozenEvaluationProfile,
    PlanBatch, PlanRecord, PlannedRolloutBinding, RolloutLedger, RolloutProgress, RolloutRecord,
    RolloutStatus, TransitionIds, ValidatedEvaluationReport, analyze_evaluation,
    commit_evaluation_claimed_transition, commit_evaluation_transition, decide,
    load_evaluation_replay, stage_and_commit_report,
};
use peritus_journal::SqliteJournal;
use peritus_types::{CommandId, EventId, Sha256Digest};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Production E3 input owner. Values submitted here remain inert until the native outbox runs.
pub struct EvaluationOperator<'a> {
    journal: &'a mut SqliteJournal,
    artifacts: &'a ArtifactStore,
}

impl<'a> EvaluationOperator<'a> {
    /// Borrows the sole writable owner and its protected artifact store.
    #[must_use]
    pub const fn new(journal: &'a mut SqliteJournal, artifacts: &'a ArtifactStore) -> Self {
        Self { journal, artifacts }
    }

    /// Freezes complete real inputs and submits the paired plan to the production scheduler.
    ///
    /// # Errors
    /// Rejects absent E1 materialization, missing task artifacts, unsupported modes, or drift.
    pub fn start(
        &mut self,
        campaign: EvaluationCampaignId,
        profile: &FrozenEvaluationProfile,
    ) -> Result<EvaluationState, DaemonError> {
        validate_profile(self.journal, self.artifacts, profile)?;
        let plan = EvaluationPlan::build(campaign, profile).map_err(error)?;
        let mut state = if let Some(state) = load_optional(self.journal, campaign)? {
            if state.profile_digest() != profile.digest()
                || state.dataset_digest() != profile.dataset().digest()
            {
                return Err(invalid("campaign identity already binds different frozen inputs"));
            }
            state
        } else {
            let ids = initial_ids(campaign, "create")?;
            let dataset_artifact = save(
                self.artifacts,
                &profile.dataset().canonical_bytes().map_err(error)?,
                ids.event_id(),
            )?;
            let profile_artifact =
                save(self.artifacts, &profile.canonical_bytes().map_err(error)?, ids.event_id())?;
            let command = EvaluationCommand::new(
                ids.command_id(),
                ids.event_id(),
                campaign,
                0,
                None,
                Sha256Digest::new([0; 32]),
                profile.digest(),
                EvaluationCommandKind::CreateCampaign {
                    revision: profile.arm(EvaluationArm::Baseline).revision(),
                    dataset_digest: profile.dataset().digest(),
                    dataset_artifact,
                    profile_artifact,
                },
            )
            .map_err(error)?;
            let transition = decide(None, &command).map_err(error)?;
            commit_evaluation_transition(self.journal, &command, &transition).map_err(error)?;
            transition.state().clone()
        };
        if state.phase().terminal() || state.phase() == EvaluationPhase::Cancelling {
            return Ok(state);
        }
        // GC does not infer nested references from a profile or image JSON document. Freeze every
        // actual executable, supplementary resource, and task root before scheduling any effect.
        let image = ArtifactDigest::from_sha256(profile.execution().image_digest());
        let catalog = super::local::LocalProcessCatalog::from_canonical_bytes(&read(
            self.artifacts,
            image,
            1024 * 1024,
        )?)
        .map_err(error)?;
        let mut inputs = catalog.artifact_roots();
        inputs.extend([image, state.dataset_artifact(), state.profile_artifact()]);
        for task in profile.dataset().tasks() {
            inputs.extend([task.candidate_input().artifact(), task.evaluator_input().artifact()]);
        }
        inputs.sort();
        inputs.dedup();
        for batch in inputs.chunks(256) {
            state = retain(self.journal, self.artifacts, campaign, batch.to_vec(), None)?;
        }
        if state.plan().is_none() {
            let mut bindings = plan
                .specs()
                .iter()
                .map(|s| PlannedRolloutBinding::new(s.id(), s.work_id(), s.request_digest()))
                .collect::<Vec<_>>();
            bindings.sort_by_key(|b| b.rollout_id());
            // Plan material is versioned structured input, never Debug text to be guessed on restart.
            let entries = plan
                .specs()
                .iter()
                .map(|s| {
                    (*s.id().as_bytes(), *s.work_id().as_bytes(), *s.request_digest().as_bytes())
                })
                .collect::<Vec<_>>();
            let bytes = serde_json::to_vec(&(
                1u16,
                *plan.id().as_bytes(),
                *plan.digest().as_bytes(),
                entries,
            ))
            .map_err(error)?;
            let artifact = save(self.artifacts, &bytes, ids(&state, "plan-batch")?.event_id())?;
            if state.rollouts().len() == 0 {
                let batch = PlanBatch::new(1, 1, artifact, bindings).map_err(error)?;
                state = ordinary(
                    self.journal,
                    &state,
                    EvaluationCommandKind::RecordPlanBatch {
                        plan_id: plan.id(),
                        plan_digest: plan.digest(),
                        batch,
                    },
                    "plan-batch",
                )?;
            }
            let rollout_count = u32::try_from(plan.specs().len())
                .map_err(|_| invalid("evaluation plan contains too many rollouts"))?;
            let root = PlanRecord::new(plan.id(), plan.digest(), artifact, rollout_count, 1)
                .map_err(error)?;
            state = ordinary(
                self.journal,
                &state,
                EvaluationCommandKind::CompletePlan { plan: root },
                "plan-complete",
            )?;
        }
        for spec in plan.dispatch_order() {
            if matches!(
                state.rollout(spec.id()).map(RolloutProgress::status),
                Some(RolloutStatus::Planned)
            ) {
                let work = spec
                    .work_spec(
                        super::scheduling::actor(&state)?,
                        *state.revision(),
                        profile.execution().resource_request().clone(),
                        profile.retry().maximum_attempts(),
                        super::scheduling::limits(profile)?,
                    )
                    .map_err(error)?;
                state = ordinary(
                    self.journal,
                    &state,
                    EvaluationCommandKind::RequestSchedule { rollout_id: spec.id(), work },
                    "request-schedule",
                )?;
            }
        }
        analyze(self.journal, self.artifacts, state)
    }

    /// Cancels before more external effects; native workers conserve observed late results.
    pub fn cancel(
        &mut self,
        campaign: EvaluationCampaignId,
        reason: Sha256Digest,
    ) -> Result<EvaluationState, DaemonError> {
        let state = load(self.journal, campaign)?;
        if state.phase().terminal() {
            return Ok(state);
        }
        let state = if state.phase() == EvaluationPhase::Cancelling {
            state
        } else {
            ordinary(
                self.journal,
                &state,
                EvaluationCommandKind::CancelCampaign { reason_digest: reason },
                "cancel",
            )?
        };
        if cancellation_ready(&state) {
            ordinary(
                self.journal,
                &state,
                EvaluationCommandKind::CompleteCancellation,
                "cancel-complete",
            )
        } else {
            Ok(state)
        }
    }

    /// Returns complete replayed state; this does not reinterpret observations.
    pub fn show(&self, campaign: EvaluationCampaignId) -> Result<EvaluationState, DaemonError> {
        load(self.journal, campaign)
    }

    /// Reads a retained native failure diagnostic, if this campaign failed.
    ///
    /// # Errors
    /// Returns storage errors for legacy failures without retained diagnostic bytes or drift.
    pub fn failure_diagnostic(
        &self,
        campaign: EvaluationCampaignId,
    ) -> Result<Option<Vec<u8>>, DaemonError> {
        let state = self.show(campaign)?;
        state
            .failure()
            .map(|failure| {
                read(self.artifacts, ArtifactDigest::from_sha256(failure.digest()), 32768)
            })
            .transpose()
    }
}

pub(crate) fn cancellation_ready(state: &EvaluationState) -> bool {
    state.phase() == EvaluationPhase::Cancelling
        && state.rollouts().all(|(_, p)| {
            !matches!(
                p.status(),
                RolloutStatus::Scheduling
                    | RolloutStatus::Scheduled { .. }
                    | RolloutStatus::Running { .. }
            )
        })
}

/// Appends an actual artifact roster in its own immutable C0 batch, never backfilling an old batch.
/// Content availability is evidence of finalized bytes, not a task verdict or effect authority.
pub(crate) fn retain(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    campaign: EvaluationCampaignId,
    mut artifacts: Vec<ArtifactDigest>,
    claim: Option<EvaluationDirectiveClaim>,
) -> Result<EvaluationState, DaemonError> {
    artifacts.sort();
    artifacts.dedup();
    let state = load(journal, campaign)?;
    if artifacts.is_empty() {
        return Ok(state);
    }
    if artifacts.len() > 256 {
        return Err(invalid("physical artifact roster exceeds its independent bound"));
    }
    for artifact in &artifacts {
        store.verify(*artifact).map_err(error)?;
    }
    let replay = load_evaluation_replay(journal, campaign).map_err(error)?;
    if replay.events().iter().any(|event| matches!(event.kind(),
        EvaluationEventKind::Accepted(EvaluationCommandKind::RecordArtifacts { artifacts: retained }) if retained == &artifacts)) {
        return Ok(state);
    }
    let command =
        command(&state, EvaluationCommandKind::RecordArtifacts { artifacts }, "retain-artifacts")?;
    let transition = decide(Some(&state), &command).map_err(error)?;
    if let Some(claim) = claim {
        commit_evaluation_claimed_transition(journal, &command, &transition, claim)
            .map_err(error)?;
    } else {
        commit_evaluation_transition(journal, &command, &transition).map_err(error)?;
    }
    Ok(transition.state().clone())
}

pub(crate) fn load_optional(
    journal: &SqliteJournal,
    campaign: EvaluationCampaignId,
) -> Result<Option<EvaluationState>, DaemonError> {
    load_evaluation_replay(journal, campaign).and_then(|r| r.rebuild()).map_err(error)
}
pub(crate) fn load(
    journal: &SqliteJournal,
    campaign: EvaluationCampaignId,
) -> Result<EvaluationState, DaemonError> {
    load_optional(journal, campaign)?.ok_or_else(|| invalid("evaluation campaign is absent"))
}

pub(crate) fn profile(
    store: &ArtifactStore,
    state: &EvaluationState,
) -> Result<FrozenEvaluationProfile, DaemonError> {
    let bytes = read(store, state.profile_artifact(), EvaluationLimits::MAX_STATE_BYTES)?;
    let profile = FrozenEvaluationProfile::from_canonical_bytes(&bytes).map_err(error)?;
    if profile.digest() != state.profile_digest()
        || profile.dataset().digest() != state.dataset_digest()
        || read(store, state.dataset_artifact(), EvaluationLimits::MAX_STATE_BYTES)?
            != profile.dataset().canonical_bytes().map_err(error)?
    {
        return Err(invalid("retained profile or dataset differs from its committed campaign"));
    }
    let plan = EvaluationPlan::build(state.campaign_id(), &profile).map_err(error)?;
    if let Some(retained) = state.plan() {
        if retained.id() != plan.id()
            || retained.digest() != plan.digest()
            || retained.expected_rollouts() as usize != plan.specs().len()
        {
            return Err(invalid("recovered rollout plan differs from its frozen inputs"));
        }
        for spec in plan.specs() {
            if state.rollout(spec.id()).is_none_or(|p| {
                p.binding()
                    != PlannedRolloutBinding::new(spec.id(), spec.work_id(), spec.request_digest())
            }) {
                return Err(invalid("retained rollout binding differs from the recovered plan"));
            }
        }
    }
    Ok(profile)
}

pub(crate) fn validate_profile(
    journal: &SqliteJournal,
    store: &ArtifactStore,
    profile: &FrozenEvaluationProfile,
) -> Result<(), DaemonError> {
    if profile.execution().mode() != EvaluationExecutionMode::LocalProcess {
        return Err(invalid(
            "native local evaluation requires an explicitly frozen LocalProcess mode",
        ));
    }
    let catalog = super::local::LocalProcessCatalog::from_canonical_bytes(&read(
        store,
        ArtifactDigest::from_sha256(profile.execution().image_digest()),
        1024 * 1024,
    )?)
    .map_err(error)?;
    catalog.validate_task_isolation(profile).map_err(error)?;
    for (arm, program) in [
        (EvaluationArm::Baseline, catalog.baseline()),
        (EvaluationArm::Candidate, catalog.candidate()),
    ] {
        let (revision, _) =
            super::evolution::resolve_materialized_arm(journal, store, profile.arm(arm))
                .map_err(error)?;
        if !revision.graph().declarations().iter().any(|d| {
            d.executable_artifact_digest()
                .is_some_and(|root| root.as_bytes() == program.artifact().as_bytes())
        }) {
            return Err(invalid(
                "native executable is not declared by its exact materialized E1 arm",
            ));
        }
        if store.verify(program.artifact()).map_err(error)?.size() != program.size() {
            return Err(invalid("native executable size differs from the frozen image manifest"));
        }
    }
    let verifier = catalog.evaluator().verifier_digest().map_err(error)?;
    for task in profile.dataset().tasks() {
        if task.evaluator_input().verifier_digest() != verifier {
            return Err(invalid(
                "sealed task verifier differs from the frozen evaluator executable",
            ));
        }
        for (digest, size) in [
            (task.candidate_input().artifact(), task.candidate_input().byte_length()),
            (task.evaluator_input().artifact(), task.evaluator_input().byte_length()),
        ] {
            if store.verify(digest).map_err(error)?.size() != size {
                return Err(invalid("task artifact size differs from its frozen descriptor"));
            }
        }
    }
    // The immutable local image manifest binds the executable catalog, support files and limits.
    Ok(())
}

pub(crate) fn ledger(
    store: &ArtifactStore,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
) -> Result<RolloutLedger, DaemonError> {
    let plan = EvaluationPlan::build(state.campaign_id(), profile).map_err(error)?;
    let mut ledger = RolloutLedger::from_plan(&plan, profile.retry().maximum_attempts());
    for spec in plan.specs() {
        let progress =
            state.rollout(spec.id()).ok_or_else(|| invalid("planned rollout is missing"))?;
        if let RolloutStatus::Settled(terminal) = progress.status() {
            let bytes = read(store, terminal.artifact(), EvaluationLimits::MAX_STATE_BYTES)?;
            if bytes.len() as u64 != terminal.artifact_bytes() {
                return Err(invalid("terminal artifact size differs"));
            }
            let record = RolloutRecord::from_canonical_bytes(&bytes).map_err(error)?;
            record.validate_spec(spec).map_err(error)?;
            if record.digest() != terminal.record_digest() {
                return Err(invalid("terminal record digest differs"));
            }
            ledger.record_attempt(spec.id(), record.attempt()).map_err(error)?;
            ledger.settle(record).map_err(error)?;
        }
    }
    Ok(ledger)
}

pub(crate) fn report(
    store: &ArtifactStore,
    state: &EvaluationState,
) -> Result<ValidatedEvaluationReport, DaemonError> {
    let profile = profile(store, state)?;
    let plan = EvaluationPlan::build(state.campaign_id(), &profile).map_err(error)?;
    let ledger = ledger(store, state, &profile)?;
    let analysis = analyze_evaluation(&plan, &profile, &ledger).map_err(error)?;
    EvaluationReport::new(
        state.campaign_id(),
        profile.dataset().digest(),
        profile.digest(),
        plan.id(),
        plan.digest(),
        analysis,
        None,
    )
    .and_then(EvaluationReport::validate)
    .map_err(error)
}

pub(crate) fn analyze(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    mut state: EvaluationState,
) -> Result<EvaluationState, DaemonError> {
    if state.phase().terminal()
        || state.phase() == EvaluationPhase::ReportReady
        || !state.counts().complete()
    {
        return Ok(state);
    }
    let report = report(store, &state)?;
    if state.phase() != EvaluationPhase::Analyzing {
        state = ordinary(
            journal,
            &state,
            EvaluationCommandKind::StartAnalysis { counts: state.counts() },
            "analysis-start",
        )?;
    }
    if state.analysis_digest().is_none() {
        let bytes =
            report.report().analysis().canonical_bytes(state.profile_digest()).map_err(error)?;
        let artifact = save(store, &bytes, ids(&state, "analysis-complete")?.event_id())?;
        state = ordinary(
            journal,
            &state,
            EvaluationCommandKind::CompleteAnalysis {
                analysis_digest: report.report().analysis().digest(),
                artifact,
                artifact_bytes: bytes.len() as u64,
            },
            "analysis-complete",
        )?;
    }
    let (_, committed) =
        stage_and_commit_report(journal, store, &state, &report, ids(&state, "report-complete")?)
            .map_err(error)?;
    Ok(committed.state().clone())
}

pub(crate) fn ordinary(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    kind: EvaluationCommandKind,
    label: &str,
) -> Result<EvaluationState, DaemonError> {
    let command = command(state, kind, label)?;
    let transition = decide(Some(state), &command).map_err(error)?;
    commit_evaluation_transition(journal, &command, &transition).map_err(error)?;
    Ok(transition.state().clone())
}
pub(crate) fn command(
    state: &EvaluationState,
    kind: EvaluationCommandKind,
    label: &str,
) -> Result<EvaluationCommand, DaemonError> {
    let ids = ids(state, label)?;
    EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        kind,
    )
    .map_err(error)
}
pub(crate) fn ids(state: &EvaluationState, label: &str) -> Result<TransitionIds, DaemonError> {
    let mut bytes = state.sequence().to_be_bytes().to_vec();
    bytes.extend_from_slice(label.as_bytes());
    initial_ids(state.campaign_id(), &format!("{bytes:?}"))
}
fn initial_ids(campaign: EvaluationCampaignId, label: &str) -> Result<TransitionIds, DaemonError> {
    let mut bytes = b"peritus.native.evaluation.command.v1\0".to_vec();
    bytes.extend_from_slice(campaign.as_bytes());
    bytes.extend_from_slice(label.as_bytes());
    let command = CommandId::new(identity(&bytes)).map_err(error)?;
    bytes.extend_from_slice(b"event");
    Ok(TransitionIds::new(command, EventId::new(identity(&bytes)).map_err(error)?))
}
pub(crate) fn identity(bytes: &[u8]) -> [u8; 16] {
    let digest = peritus_codec::sha256(bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    id
}
pub(crate) fn save(
    store: &ArtifactStore,
    bytes: &[u8],
    event: EventId,
) -> Result<ArtifactDigest, DaemonError> {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let mut writer = store
        .begin_write(WriteRequest::new(
            digest,
            bytes.len() as u64,
            (bytes.len() as u64).max(1),
            MediaType::new("application/octet-stream").map_err(error)?,
            EncryptionMetadata::unencrypted(),
            event,
        ))
        .map_err(error)?;
    writer.write_chunk(bytes).map_err(error)?;
    writer.finalize().map(FinalizedArtifact::digest).map_err(error)
}
pub(crate) fn read(
    store: &ArtifactStore,
    digest: ArtifactDigest,
    maximum: u64,
) -> Result<Vec<u8>, DaemonError> {
    let metadata = store.verify(digest).map_err(error)?;
    if metadata.size() > maximum {
        return Err(invalid("evaluation artifact exceeds its independent read bound"));
    }
    let mut reader = store.open_read(digest).map_err(error)?;
    let mut bytes = Vec::new();
    while let Some(chunk) =
        reader.read_chunk(metadata.size().clamp(1, 65536) as usize).map_err(error)?
    {
        bytes.extend_from_slice(chunk.bytes());
        if bytes.len() as u64 > maximum {
            return Err(invalid("evaluation artifact exceeds its independent read bound"));
        }
    }
    if bytes.len() as u64 != metadata.size() || peritus_codec::sha256(&bytes) != digest.sha256() {
        return Err(invalid("evaluation artifact content differs from its digest"));
    }
    Ok(bytes)
}
pub(crate) fn error<T>(source: T) -> DaemonError
where
    T: core::fmt::Debug,
{
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "native evaluation",
        format!("native evaluation owner rejected the operation: {source:?}"),
    )
}
pub(crate) fn invalid(detail: &str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "native evaluation",
        detail.to_owned(),
    )
}
