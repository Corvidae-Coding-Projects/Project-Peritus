//! Atomic cancellation settlement, outbox acknowledgement, and restart coverage.

mod support;

use std::time::Duration;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_eval::{
    EvaluationCommand, EvaluationCommandKind, EvaluationPhase, EvaluationPlan, EvaluationState,
    PlanBatch, PlanRecord, PlannedRolloutBinding, ScheduleDirectiveClaim,
    commit_evaluation_settlement, commit_evaluation_transition, decide, load_evaluation_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerLimits,
};
use peritus_types::{ActorId, CommandId, EventId};

use support::{bytes, campaign_id, digest, frozen_profile, revision};

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one linear scenario demonstrates atomic settlement and restart semantics"
)]
fn cancellation_settlement_atomically_acknowledges_the_existing_effect_and_restarts() {
    let stores = Stores::open();
    let Stores { temporary, mut journal, artifacts } = stores;
    let dataset_artifact = finalize(&artifacts, b"dataset", 130);
    let profile_artifact = finalize(&artifacts, b"profile", 131);
    let batch_artifact = finalize(&artifacts, b"plan-batch", 132);
    let root_artifact = finalize(&artifacts, b"plan-root", 133);
    let profile = frozen_profile();
    let plan = EvaluationPlan::build(campaign_id(), &profile).expect("plan");

    let genesis = EvaluationCommand::new(
        CommandId::new(bytes(140)).expect("command"),
        EventId::new(bytes(141)).expect("event"),
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
    commit_evaluation_transition(&mut journal, &genesis, &transition).expect("commit create");
    let mut state = transition.state().clone();

    let mut bindings: Vec<_> = plan
        .specs()
        .iter()
        .map(|spec| PlannedRolloutBinding::new(spec.id(), spec.work_id(), spec.request_digest()))
        .collect();
    bindings.sort_unstable_by_key(|binding| binding.rollout_id());
    state = commit_ordinary(
        &mut journal,
        &state,
        142,
        EvaluationCommandKind::RecordPlanBatch {
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            batch: PlanBatch::new(1, 1, batch_artifact, bindings).expect("batch"),
        },
        profile.digest(),
    );
    state = commit_ordinary(
        &mut journal,
        &state,
        144,
        EvaluationCommandKind::CompletePlan {
            plan: PlanRecord::new(plan.id(), plan.digest(), root_artifact, 8, 1)
                .expect("plan record"),
        },
        profile.digest(),
    );

    let rollout = plan.specs()[0].id();
    let work = plan.specs()[0]
        .work_spec(
            ActorId::new(bytes(146)).expect("owner"),
            revision(),
            resources(),
            3,
            scheduler_limits(),
        )
        .expect("work");
    state = commit_ordinary(
        &mut journal,
        &state,
        147,
        EvaluationCommandKind::RequestSchedule { rollout_id: rollout, work },
        profile.digest(),
    );
    state = commit_ordinary(
        &mut journal,
        &state,
        149,
        EvaluationCommandKind::CancelCampaign { reason_digest: digest(150) },
        profile.digest(),
    );

    let message = journal.claim_outbox(1, 20).expect("claim query").expect("schedule effect");
    let claim = ScheduleDirectiveClaim::from_message(&message).expect("checked schedule claim");
    assert_eq!(claim.directive().rollout_id(), rollout);
    let settlement = command(
        &state,
        151,
        EvaluationCommandKind::SettleCancellation {
            rollout_id: rollout,
            observation_digest: digest(152),
        },
        profile.digest(),
    );
    let transition = decide(Some(&state), &settlement).expect("cancellation settlement");
    commit_evaluation_settlement(&mut journal, &settlement, &transition, claim)
        .expect("atomic cancellation acknowledgement");
    state = transition.state().clone();
    assert!(journal.claim_outbox(21, 30).expect("outbox query").is_none());

    state = commit_ordinary(
        &mut journal,
        &state,
        153,
        EvaluationCommandKind::CompleteCancellation,
        profile.digest(),
    );
    assert_eq!(state.phase(), EvaluationPhase::Cancelled);
    drop(journal);

    let journal = SqliteJournal::open(
        temporary.path().join("shared.sqlite3"),
        StoreId::new(bytes(160)).expect("store"),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
    )
    .expect("reopen journal");
    let rebuilt = load_evaluation_replay(&journal, campaign_id())
        .expect("load replay")
        .rebuild()
        .expect("rebuild")
        .expect("state");
    assert_eq!(rebuilt, state);
    assert_eq!(rebuilt.phase(), EvaluationPhase::Cancelled);
}

fn commit_ordinary(
    journal: &mut SqliteJournal,
    prior: &EvaluationState,
    seed: u8,
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
    seed: u8,
    kind: EvaluationCommandKind,
    profile: peritus_eval::ProfileDigest,
) -> EvaluationCommand {
    EvaluationCommand::new(
        CommandId::new(bytes(seed)).expect("command"),
        EventId::new(bytes(seed.saturating_add(1))).expect("event"),
        campaign_id(),
        prior.sequence(),
        Some(prior.last_event_id()),
        prior.state_digest(),
        profile,
        kind,
    )
    .expect("command")
}

struct Stores {
    temporary: tempfile::TempDir,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
}

impl Stores {
    fn open() -> Self {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let database = temporary.path().join("shared.sqlite3");
        let journal = SqliteJournal::open(
            &database,
            StoreId::new(bytes(160)).expect("store"),
            SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
        )
        .expect("journal");
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temporary.path().join("artifacts"), 1_048_576, 8 * 1_048_576)
                .expect("config")
                .with_database_path(database)
                .expect("shared database"),
        )
        .expect("artifact store");
        Self { temporary, journal, artifacts }
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
