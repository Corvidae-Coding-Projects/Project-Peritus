//! Serialized E3/F0 claim ownership. External processes run outside this owner.

use peritus_artifact_store::ArtifactStore;
use peritus_eval::*;
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_journal::SqliteJournal;

use super::{evaluation as e, scheduling};
use crate::DaemonError;

pub(crate) struct EvaluationWork {
    pub(crate) committed: CommittedEvaluationTransition,
    pub(crate) profile: FrozenEvaluationProfile,
    pub(crate) spec: RolloutSpec,
}

pub(crate) fn schedule(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    claim: ScheduleDirectiveClaim,
) -> Result<(), DaemonError> {
    let state = e::load(journal, claim.directive().campaign_id())?;
    if settle_terminal(journal, &state, &EvaluationDirectiveClaim::Schedule(claim.clone()))? {
        return Ok(());
    }
    let profile = e::profile(store, &state)?;
    e::validate_profile(journal, store, &profile)?;
    let spec = spec(&state, &profile, claim.directive().rollout_id())?;
    if state.phase() == EvaluationPhase::Cancelling
        || matches!(claim.directive().kind(), ScheduleDirectiveKind::Cancel(_))
    {
        return settle_cancel(journal, &state, &profile, &spec, claim.into());
    }
    let ScheduleDirectiveKind::Submit(work) = claim.directive().kind() else {
        return Err(e::invalid("schedule effect is not a submit"));
    };
    let acknowledgement_digest = scheduling::schedule(journal, &state, &profile, &spec, work)?;
    let command = e::command(
        &state,
        EvaluationCommandKind::RecordSchedule { rollout_id: spec.id(), acknowledgement_digest },
        "record-schedule",
    )?;
    let transition = decide(Some(&state), &command).map_err(e::error)?;
    commit_evaluation_settlement(journal, &command, &transition, claim).map_err(e::error)?;
    Ok(())
}

pub(crate) fn begin(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    claim: ExecutionDirectiveClaim,
) -> Result<Option<EvaluationWork>, DaemonError> {
    let state = e::load(journal, claim.directive().campaign_id())?;
    if settle_terminal(journal, &state, &EvaluationDirectiveClaim::Execution(claim.clone()))? {
        return Ok(None);
    }
    let profile = e::profile(store, &state)?;
    e::validate_profile(journal, store, &profile)?;
    let spec = spec(&state, &profile, claim.directive().rollout_id())?;
    if state.phase() == EvaluationPhase::Cancelling
        || claim.directive().kind() == ExecutionDirectiveKind::Cancel
    {
        settle_cancel(journal, &state, &profile, &spec, claim.into())?;
        return Ok(None);
    }
    if !matches!(claim.directive().kind(), ExecutionDirectiveKind::Execute { request_digest } if request_digest == spec.request_digest())
    {
        return Err(e::invalid(
            "native execution request differs from its recovered specification",
        ));
    }
    if matches!(
        state.rollout(spec.id()).map(RolloutProgress::status),
        Some(RolloutStatus::Running { .. })
    ) {
        return Err(e::invalid(
            "interrupted running rollout needs exact owned-process recovery before any new attempt",
        ));
    }
    scheduling::begin(journal, &state, &profile, &spec)?;
    let attempt = state
        .rollout(spec.id())
        .ok_or_else(|| e::invalid("rollout is absent"))?
        .attempts_retained()
        .saturating_add(1);
    let command = e::command(
        &state,
        EvaluationCommandKind::StartRollout {
            rollout_id: spec.id(),
            attempt,
            started_at_tick: state.sequence().saturating_add(1),
        },
        "start-rollout",
    )?;
    let transition = decide(Some(&state), &command).map_err(e::error)?;
    let committed = EvaluationRuntime::new(journal)
        .commit_claimed(&command, &transition, claim)
        .map_err(e::error)?;
    Ok(Some(EvaluationWork { committed, profile, spec }))
}

pub(crate) fn finish(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    processes: &peritus_process::ProcessStore,
    state_root: &std::path::Path,
    claim: ExecutionDirectiveClaim,
    mut record: RolloutRecord,
) -> Result<(), DaemonError> {
    let mut state = e::load(journal, claim.directive().campaign_id())?;
    let profile = e::profile(store, &state)?;
    let spec = spec(&state, &profile, claim.directive().rollout_id())?;
    record.validate_spec(&spec).map_err(e::error)?;
    if matches!(
        record.outcome(),
        RolloutOutcome::TaskPassed { .. } | RolloutOutcome::TaskFailed { .. }
    ) {
        let trace = super::evolution::import_local_trace(
            journal, store, processes, state_root, &profile, &spec, &record,
        )
        .map_err(e::error)?;
        record = record.with_trace_root(trace.manifest);
    }
    if matches!(state.phase(), EvaluationPhase::Cancelling | EvaluationPhase::Cancelled) {
        record = record.mark_late_after_cancellation().map_err(e::error)?;
    }
    let bytes = record.canonical_bytes();
    let artifact = e::save(store, &bytes, e::ids(&state, "settle-rollout")?.event_id())?;
    let mut physical = vec![artifact];
    physical.extend([record.trace_root(), record.evidence_root()].into_iter().flatten());
    if let Some(bundle) = record.evidence_root() {
        let bytes = e::read(store, bundle, 4 * 1024 * 1024)?;
        if let Ok((version, rollout, attempt, roots)) =
            serde_json::from_slice::<(u16, [u8; 16], u16, Vec<[u8; 32]>)>(&bytes)
        {
            if version != 1
                || rollout != *spec.id().as_bytes()
                || attempt != record.attempt().number()
                || roots.len() > 6
            {
                return Err(e::invalid(
                    "physical result bundle differs from its actual admitted rollout",
                ));
            }
            physical.extend(roots.into_iter().map(peritus_artifact_store::ArtifactDigest::new));
        }
    }
    if let RolloutOutcome::InfrastructureFailed { failure_digest, .. }
    | RolloutOutcome::Ambiguous { observation_digest: failure_digest } = record.outcome()
    {
        let root = peritus_artifact_store::ArtifactDigest::from_sha256(failure_digest);
        if store.verify(root).is_ok() {
            physical.push(root);
        }
    }
    state = e::retain(journal, store, state.campaign_id(), physical, Some(claim.clone().into()))?;
    if state.phase().terminal() {
        // Logical terminal state stays terminal. A genuinely started late child must still
        // release its reservation and retain its physical observation before its exact ACK.
        if matches!(
            state.rollout(spec.id()).map(RolloutProgress::status),
            Some(RolloutStatus::Running { .. })
        ) {
            scheduling::finish(journal, &state, &spec, &record)?;
        }
        settle_terminal(journal, &state, &EvaluationDirectiveClaim::Execution(claim))?;
        return Ok(());
    }
    if record.outcome() == RolloutOutcome::Cancelled {
        if state.phase() != EvaluationPhase::Cancelling {
            return Err(e::invalid(
                "cancelled native process has no durable campaign cancellation",
            ));
        }
        return settle_cancel(journal, &state, &profile, &spec, claim.into());
    }
    scheduling::finish(journal, &state, &spec, &record)?;
    if state.phase() == EvaluationPhase::Cancelling {
        // Preserve the actual late verdict artifact, while cancellation owns logical accounting.
        return settle_cancel(journal, &state, &profile, &spec, claim.into());
    }
    let class = match record.outcome() {
        RolloutOutcome::TaskPassed { .. } => RolloutTerminalClass::Passed,
        RolloutOutcome::TaskFailed { .. } => RolloutTerminalClass::TaskFailed,
        RolloutOutcome::InfrastructureFailed { .. } => RolloutTerminalClass::InfrastructureFailed,
        RolloutOutcome::Ambiguous { .. } => RolloutTerminalClass::Ambiguous,
        RolloutOutcome::Cancelled => unreachable!("cancelled records were settled above"),
    };
    let terminal = TerminalRecordRef::new(
        class,
        record.digest(),
        artifact,
        bytes.len() as u64,
        record.attempt().number(),
    )
    .map_err(e::error)?;
    let command = e::command(
        &state,
        EvaluationCommandKind::SettleRollout { rollout_id: spec.id(), terminal },
        "settle-rollout",
    )?;
    let transition = decide(Some(&state), &command).map_err(e::error)?;
    commit_evaluation_settlement(journal, &command, &transition, claim).map_err(e::error)?;
    if transition.state().phase() == EvaluationPhase::Cancelling {
        complete_cancel(journal, transition.state())?;
    } else {
        e::analyze(journal, store, transition.state().clone())?;
    }
    Ok(())
}

pub(crate) fn publish(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    database: &std::path::Path,
    claim: PublicationDirectiveClaim,
) -> Result<(), DaemonError> {
    let state = e::load(journal, claim.directive().campaign_id())?;
    if settle_terminal(journal, &state, &EvaluationDirectiveClaim::Publication(claim.clone()))? {
        return Ok(());
    }
    let report = e::report(store, &state)?;
    let retained =
        state.report().ok_or_else(|| e::invalid("publication has no retained report"))?;
    if e::read(store, retained.artifact(), EvaluationLimits::MAX_STATE_BYTES)? != report.bytes() {
        return Err(e::invalid(
            "published report bytes differ from recomputed actual terminal analysis",
        ));
    }
    let artifact =
        finalize_report_artifact(store, &report, state.last_event_id()).map_err(e::error)?;
    let mut evidence =
        EvidenceStore::open(database, EvidenceStoreOptions::default()).map_err(e::error)?;
    // Publication uses the report-producing C0 position retained by the actual native claim.
    let producing_position = claim.producing_position();
    publish_claimed_report(
        journal,
        &mut evidence,
        store,
        &state,
        &report,
        artifact,
        producing_position,
        claim,
        e::ids(&state, "publish-report")?,
    )
    .map_err(e::error)?;
    Ok(())
}

pub(crate) fn fail(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    processes: &peritus_process::ProcessStore,
    config: &crate::DaemonConfig,
    claim: EvaluationDirectiveClaim,
    mut detail: String,
) -> Result<(), DaemonError> {
    let campaign = match &claim {
        EvaluationDirectiveClaim::Schedule(c) => c.directive().campaign_id(),
        EvaluationDirectiveClaim::Execution(c) => c.directive().campaign_id(),
        EvaluationDirectiveClaim::Publication(c) => c.directive().campaign_id(),
    };
    let state = e::load(journal, campaign)?;
    let running_claim = matches!(&claim, EvaluationDirectiveClaim::Execution(c) if
        matches!(state.rollout(c.directive().rollout_id()).map(RolloutProgress::status), Some(RolloutStatus::Running { .. })));
    if !running_claim && settle_terminal(journal, &state, &claim)? {
        return Ok(());
    }
    // Retain the bounded actual diagnostic, rather than an unrecoverable hash of an error.
    if detail.len() > 16384 {
        let mut boundary = 16384;
        while !detail.is_char_boundary(boundary) {
            boundary -= 1;
        }
        detail.truncate(boundary);
        detail.push_str(" [diagnostic truncated at 16 KiB]");
    }
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1, "campaign": campaign.as_bytes(), "detail": detail
    }))
    .map_err(e::error)?;
    let diagnostic = e::save(store, &bytes, e::ids(&state, "native-failure")?.event_id())?;
    let state = e::retain(journal, store, campaign, vec![diagnostic], Some(claim.clone()))?;
    if let EvaluationDirectiveClaim::Execution(execution) = &claim {
        if matches!(
            state.rollout(execution.directive().rollout_id()).map(RolloutProgress::status),
            Some(RolloutStatus::Running { .. })
        ) {
            // A started reservation cannot disappear into a campaign failure. Reconcile exact
            // birth-bound children and settle actual infrastructure failure or ambiguity first.
            return super::recovery::recover_claim(
                journal,
                store,
                processes,
                config,
                execution,
                Some(diagnostic),
            );
        }
    }
    if state.rollouts().all(|(_, p)| !matches!(p.status(), RolloutStatus::Running { .. })) {
        // No external work is owned at this boundary. Retire queued reservations honestly;
        // this does not manufacture candidate outcomes for the failed campaign.
        if let Ok(profile) = e::profile(store, &state) {
            for spec in EvaluationPlan::build(campaign, &profile).map_err(e::error)?.specs() {
                scheduling::cancel(journal, &state, &profile, spec)?;
            }
        }
    }
    let command = e::command(
        &state,
        EvaluationCommandKind::FailCampaign {
            failure: CampaignFailure::new(CampaignFailureCode::Binding, diagnostic.sha256()),
        },
        "native-failure",
    )?;
    let transition = decide(Some(&state), &command).map_err(e::error)?;
    commit_evaluation_settlement(journal, &command, &transition, claim).map_err(e::error)?;
    Ok(())
}

fn settle_terminal(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    claim: &EvaluationDirectiveClaim,
) -> Result<bool, DaemonError> {
    if !state.phase().terminal() {
        return Ok(false);
    }
    // The owning terminal is already durable; no effect may resume or gain candidate authority.
    journal
        .acknowledge_outbox(claim.outbox_id().map_err(e::error)?, claim.claim_fence())
        .map_err(e::error)?;
    Ok(true)
}
fn settle_cancel(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    spec: &RolloutSpec,
    claim: EvaluationDirectiveClaim,
) -> Result<(), DaemonError> {
    let observation_digest = scheduling::cancel(journal, state, profile, spec)?;
    let command = e::command(
        state,
        EvaluationCommandKind::SettleCancellation { rollout_id: spec.id(), observation_digest },
        "settle-cancel",
    )?;
    let transition = decide(Some(state), &command).map_err(e::error)?;
    commit_evaluation_settlement(journal, &command, &transition, claim).map_err(e::error)?;
    complete_cancel(journal, transition.state())
}
fn complete_cancel(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
) -> Result<(), DaemonError> {
    if e::cancellation_ready(state) {
        e::ordinary(
            journal,
            state,
            EvaluationCommandKind::CompleteCancellation,
            "cancel-complete",
        )?;
    }
    Ok(())
}
fn spec(
    state: &EvaluationState,
    profile: &FrozenEvaluationProfile,
    id: RolloutId,
) -> Result<RolloutSpec, DaemonError> {
    EvaluationPlan::build(state.campaign_id(), profile)
        .map_err(e::error)?
        .specs()
        .iter()
        .find(|s| s.id() == id)
        .cloned()
        .ok_or_else(|| e::invalid("directive names a rollout outside the frozen plan"))
}
