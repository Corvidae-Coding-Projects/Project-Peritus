//! Full schedule, execution, analysis, report, evidence, and replay integration coverage.

mod support;

use std::time::Duration;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_eval::{
    EvaluationArm, EvaluationCommand, EvaluationCommandKind, EvaluationPhase, EvaluationPlan,
    EvaluationReport, EvaluationState, ExecutionDirectiveClaim, NeverCancelled, PlanBatch,
    PlanRecord, PlannedRolloutBinding, PublicationDirectiveClaim, ResultDigest, RolloutLedger,
    RolloutRecord, RolloutTerminalClass, ScheduleDirectiveClaim, TerminalRecordRef, TransitionIds,
    analyze_evaluation, commit_evaluation_claimed_transition, commit_evaluation_settlement,
    commit_evaluation_transition, decide, execute_rollout, load_evaluation_replay,
    publish_claimed_report, stage_and_commit_report,
};
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerLimits,
};
use peritus_types::{ActorId, CommandId, EventId};

use support::{FixturePort, PortMode, bytes, campaign_id, digest, frozen_profile, revision};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one linear end-to-end lifecycle keeps the publication replay evidence auditable"
)]
fn complete_campaign_publishes_evidence_and_replays_without_duplicate_effects() {
    let mut stores = Stores::open();
    let dataset_artifact = finalize(&stores.artifacts, b"dataset", 2);
    let profile_artifact = finalize(&stores.artifacts, b"profile", 3);
    let batch_artifact = finalize(&stores.artifacts, b"plan-batch", 4);
    let root_artifact = finalize(&stores.artifacts, b"plan-root", 5);
    let analysis_artifact = finalize(&stores.artifacts, b"analysis", 6);
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");
    let mut seed = 10_u8;

    let genesis = EvaluationCommand::new(
        CommandId::new(bytes(take(&mut seed))).expect("command"),
        EventId::new(bytes(take(&mut seed))).expect("event"),
        campaign_id(),
        0,
        None,
        digest(0),
        profile.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: revision(),
            dataset_digest: profile.dataset().digest(),
            dataset_artifact,
            profile_artifact,
        },
    )
    .expect("genesis");
    let transition = decide(None, &genesis).expect("create");
    commit_evaluation_transition(&mut stores.journal, &genesis, &transition)
        .expect("commit create");
    let mut state = transition.state().clone();

    let mut bindings: Vec<_> = plan
        .specs()
        .iter()
        .map(|spec| PlannedRolloutBinding::new(spec.id(), spec.work_id(), spec.request_digest()))
        .collect();
    bindings.sort_unstable_by_key(|binding| binding.rollout_id());
    state = commit_ordinary(
        &mut stores.journal,
        &state,
        &mut seed,
        EvaluationCommandKind::RecordPlanBatch {
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            batch: PlanBatch::new(1, 1, batch_artifact, bindings).expect("plan batch"),
        },
        profile.digest(),
    );
    state = commit_ordinary(
        &mut stores.journal,
        &state,
        &mut seed,
        EvaluationCommandKind::CompletePlan {
            plan: PlanRecord::new(
                plan.id(),
                plan.digest(),
                root_artifact,
                u32::try_from(plan.specs().len()).expect("rollout count"),
                1,
            )
            .expect("plan record"),
        },
        profile.digest(),
    );

    let mut ledger = RolloutLedger::from_plan(&plan, 3);
    for (index, spec) in plan.specs().iter().enumerate() {
        let work = spec
            .work_spec(
                ActorId::new(bytes(180)).expect("owner"),
                revision(),
                resources(),
                3,
                scheduler_limits(),
            )
            .expect("work");
        state = commit_ordinary(
            &mut stores.journal,
            &state,
            &mut seed,
            EvaluationCommandKind::RequestSchedule { rollout_id: spec.id(), work },
            profile.digest(),
        );
        let schedule_message = stores
            .journal
            .claim_outbox(100 + u64::try_from(index).expect("tick"), 200)
            .expect("schedule claim query")
            .expect("schedule directive");
        let schedule_claim =
            ScheduleDirectiveClaim::from_message(&schedule_message).expect("schedule claim");
        let schedule = command(
            &state,
            &mut seed,
            EvaluationCommandKind::RecordSchedule {
                rollout_id: spec.id(),
                acknowledgement_digest: digest(181),
            },
            profile.digest(),
        );
        let scheduled = decide(Some(&state), &schedule).expect("schedule settlement");
        commit_evaluation_settlement(&mut stores.journal, &schedule, &scheduled, schedule_claim)
            .expect("commit schedule settlement");
        state = scheduled.state().clone();

        let execution_message = stores
            .journal
            .claim_outbox(300 + u64::try_from(index).expect("tick"), 400)
            .expect("execution claim query")
            .expect("execution directive");
        let execution_claim =
            ExecutionDirectiveClaim::from_message(&execution_message).expect("execution claim");
        let start = command(
            &state,
            &mut seed,
            EvaluationCommandKind::StartRollout {
                rollout_id: spec.id(),
                attempt: 1,
                started_at_tick: 500 + u64::try_from(index).expect("tick"),
            },
            profile.digest(),
        );
        let running = decide(Some(&state), &start).expect("start transition");
        commit_evaluation_claimed_transition(
            &mut stores.journal,
            &start,
            &running,
            execution_claim.clone(),
        )
        .expect("commit pre-effect start");
        state = running.state().clone();

        let mut port = FixturePort::new(if spec.arm() == EvaluationArm::Candidate && index == 3 {
            PortMode::TaskFail
        } else {
            PortMode::Pass
        });
        let executed = execute_rollout(&mut port, &NeverCancelled, spec, &profile, 1)
            .expect("execute rollout");
        let record = RolloutRecord::from_execution(spec, executed, None, None).expect("record");
        ledger.record_attempt(spec.id(), record.attempt()).expect("retain attempt");
        ledger.settle(record).expect("settle ledger");
        let arm_tag = match spec.arm() {
            EvaluationArm::Baseline => 1,
            EvaluationArm::Candidate => 2,
        };
        let payload = [u8::try_from(index).expect("small index"), arm_tag];
        let artifact = finalize(
            &stores.artifacts,
            &payload,
            190_u8.checked_add(u8::try_from(index).expect("small index")).expect("event seed"),
        );
        let terminal = TerminalRecordRef::new(
            match record.outcome() {
                peritus_eval::RolloutOutcome::TaskPassed { .. } => RolloutTerminalClass::Passed,
                peritus_eval::RolloutOutcome::TaskFailed { .. } => RolloutTerminalClass::TaskFailed,
                peritus_eval::RolloutOutcome::InfrastructureFailed { .. } => {
                    RolloutTerminalClass::InfrastructureFailed
                }
                peritus_eval::RolloutOutcome::Ambiguous { .. } => RolloutTerminalClass::Ambiguous,
                peritus_eval::RolloutOutcome::Cancelled => panic!("fixture does not cancel"),
            },
            record.digest(),
            artifact,
            u64::try_from(payload.len()).expect("payload size"),
            1,
        )
        .expect("terminal reference");
        let settle = command(
            &state,
            &mut seed,
            EvaluationCommandKind::SettleRollout { rollout_id: spec.id(), terminal },
            profile.digest(),
        );
        let settled = decide(Some(&state), &settle).expect("terminal transition");
        commit_evaluation_settlement(&mut stores.journal, &settle, &settled, execution_claim)
            .expect("commit terminal and acknowledgement");
        state = settled.state().clone();
    }
    assert!(state.counts().complete());
    assert!(stores.journal.claim_outbox(500, 600).expect("outbox query").is_none());

    state = commit_ordinary(
        &mut stores.journal,
        &state,
        &mut seed,
        EvaluationCommandKind::StartAnalysis { counts: state.counts() },
        profile.digest(),
    );
    let analysis = analyze_evaluation(&plan, &profile, &ledger).expect("analysis");
    state = commit_ordinary(
        &mut stores.journal,
        &state,
        &mut seed,
        EvaluationCommandKind::CompleteAnalysis {
            analysis_digest: ResultDigest::new(peritus_codec::sha256(b"analysis")),
            artifact: analysis_artifact,
            artifact_bytes: u64::try_from(b"analysis".len()).expect("analysis size"),
        },
        profile.digest(),
    );
    let report = EvaluationReport::new(
        campaign_id(),
        profile.dataset().digest(),
        profile.digest(),
        plan.id(),
        plan.digest(),
        analysis,
        None,
    )
    .expect("report")
    .validate()
    .expect("validated report");
    let report_ids = transition_ids(&mut seed);
    let (artifact, ready) = stage_and_commit_report(
        &mut stores.journal,
        &stores.artifacts,
        &state,
        &report,
        report_ids,
    )
    .expect("stage report");
    let report_position = ready.batch().last_position();
    let publication_message = stores
        .journal
        .claim_outbox(700, 800)
        .expect("publication claim query")
        .expect("publication directive");
    let publication_claim =
        PublicationDirectiveClaim::from_message(&publication_message).expect("publication claim");
    let published = publish_claimed_report(
        &mut stores.journal,
        &mut stores.evidence,
        &stores.artifacts,
        ready.state(),
        &report,
        artifact,
        report_position,
        publication_claim,
        transition_ids(&mut seed),
    )
    .expect("publish report");
    assert_eq!(published.committed().state().phase(), EvaluationPhase::Published);
    assert!(stores.journal.claim_outbox(801, 900).expect("outbox query").is_none());
    assert_eq!(
        stores.evidence.load(published.evidence().id()).expect("load evidence"),
        Some(published.evidence().clone()),
    );
    let rebuilt = load_evaluation_replay(&stores.journal, campaign_id())
        .expect("load replay")
        .rebuild()
        .expect("rebuild")
        .expect("published state");
    assert_eq!(rebuilt, published.committed().state().clone());
}

fn commit_ordinary(
    journal: &mut SqliteJournal,
    prior: &EvaluationState,
    seed: &mut u8,
    kind: EvaluationCommandKind,
    profile: peritus_eval::ProfileDigest,
) -> EvaluationState {
    let command = command(prior, seed, kind, profile);
    let transition = decide(Some(prior), &command).expect("legal transition");
    commit_evaluation_transition(journal, &command, &transition).expect("ordinary commit");
    transition.state().clone()
}

fn command(
    prior: &EvaluationState,
    seed: &mut u8,
    kind: EvaluationCommandKind,
    profile: peritus_eval::ProfileDigest,
) -> EvaluationCommand {
    EvaluationCommand::new(
        CommandId::new(bytes(take(seed))).expect("command"),
        EventId::new(bytes(take(seed))).expect("event"),
        campaign_id(),
        prior.sequence(),
        Some(prior.last_event_id()),
        prior.state_digest(),
        profile,
        kind,
    )
    .expect("command")
}

fn transition_ids(seed: &mut u8) -> TransitionIds {
    TransitionIds::new(
        CommandId::new(bytes(take(seed))).expect("command"),
        EventId::new(bytes(take(seed))).expect("event"),
    )
}

const fn take(seed: &mut u8) -> u8 {
    let value = *seed;
    *seed = seed.checked_add(1).expect("fixture identity space");
    value
}

struct Stores {
    _temporary: tempfile::TempDir,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
    evidence: EvidenceStore,
}

impl Stores {
    fn open() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let database = temporary.path().join("shared.sqlite3");
        let journal = SqliteJournal::open(
            &database,
            StoreId::new(bytes(1)).expect("store"),
            SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
        )
        .expect("journal");
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temporary.path().join("artifacts"), 1_048_576, 8 * 1_048_576)
                .expect("config")
                .with_database_path(&database)
                .expect("shared database"),
        )
        .expect("artifact store");
        let evidence = EvidenceStore::open(&database, EvidenceStoreOptions::default())
            .expect("evidence store");
        Self { _temporary: temporary, journal, artifacts, evidence }
    }
}

fn finalize(store: &ArtifactStore, payload: &[u8], event_seed: u8) -> ArtifactDigest {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(payload));
    let size = u64::try_from(payload.len()).expect("size");
    let request = WriteRequest::new(
        digest,
        size,
        size,
        MediaType::new("application/x-peritus-evaluation-fixture".to_owned()).expect("media type"),
        EncryptionMetadata::unencrypted(),
        EventId::new(bytes(event_seed)).expect("creating event"),
    );
    let mut writer = store.begin_write(request).expect("writer");
    writer.write_chunk(payload).expect("write");
    writer.finalize().expect("finalize").digest()
}

fn resources() -> ResourceVector {
    ResourceVector::new(
        vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).expect("quantity"))],
        4,
    )
    .expect("resources")
}

fn scheduler_limits() -> SchedulerLimits {
    SchedulerLimits::new(10, 10, 2, 4, 4, 2, 3, 8, 2, 1_024, 1_048_576).expect("scheduler limits")
}
