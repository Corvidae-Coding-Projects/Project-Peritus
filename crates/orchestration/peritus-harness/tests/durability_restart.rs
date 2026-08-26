//! C0 durability, restart, and committed-plan recovery tests.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]

use std::path::Path;

mod fixtures_support;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_harness::{
    DirectiveClaim, HarnessCommand, HarnessCommandKind, HarnessRuntime, MaterializationFailure,
    MaterializationFailureCode, MaterializationPlan, MaterializationReason, ObservedTarget,
    PlanCommitEvidence, PlanningOutcome, WorkspaceSnapshot, commit_harness_settlement,
    commit_harness_transition, decide, load_harness_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::{EventId, Sha256Digest};

fn store_content(root: &Path, database: &Path, content: &[u8], event_id: EventId) -> ArtifactStore {
    let store = ArtifactStore::open(
        StoreConfig::new(root, 1_024, 8_192).unwrap().with_database_path(database).unwrap(),
    )
    .unwrap();
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(content));
    let mut writer = store
        .begin_write(WriteRequest::new(
            digest,
            content.len() as u64,
            1_024,
            MediaType::new("text/plain").unwrap(),
            EncryptionMetadata::unencrypted(),
            event_id,
        ))
        .unwrap();
    writer.write_chunk(content).unwrap();
    writer.finalize().unwrap();
    store
}

#[test]
fn atomic_genesis_commit_is_idempotent_and_restart_replay_matches_checkpoint() {
    let (revision, content) = fixtures_support::genesis_fixture();
    let event_id = fixtures_support::event_id(12);
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("journal.sqlite3");
    let _artifact_store =
        store_content(&directory.path().join("artifacts"), &database, &content, event_id);

    let command = HarnessCommand::new(
        fixtures_support::command_id(11),
        event_id,
        revision.harness_id(),
        0,
        None,
        Sha256Digest::new([0; 32]),
        HarnessCommandKind::RegisterGenesis { revision: revision.clone() },
    )
    .unwrap();
    let transition = decide(None, &command).unwrap();
    let store_id = StoreId::new(fixtures_support::bytes(99)).unwrap();
    let mut journal =
        SqliteJournal::open(&database, store_id, SqliteJournalOptions::default()).unwrap();
    let committed = commit_harness_transition(&mut journal, &command, &transition).unwrap();
    let resolved = commit_harness_transition(&mut journal, &command, &transition).unwrap();
    assert_eq!(committed.batch_hash(), resolved.batch_hash());
    assert_eq!(committed.artifact_dependencies().len(), 1);
    drop(journal);

    let restarted =
        SqliteJournal::open(&database, store_id, SqliteJournalOptions::default()).unwrap();
    let replay = load_harness_replay(&restarted, revision.harness_id()).unwrap();
    assert_eq!(replay.rebuild().unwrap(), Some(transition.state().clone()));
}

#[test]
fn checked_restart_recreates_plan_and_atomically_settles_exact_claim() {
    let (revision, content) = fixtures_support::genesis_fixture();
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("journal.sqlite3");
    let genesis_event = fixtures_support::event_id(22);
    let artifact_store =
        store_content(&directory.path().join("artifacts"), &database, &content, genesis_event);

    let genesis = HarnessCommand::new(
        fixtures_support::command_id(21),
        genesis_event,
        revision.harness_id(),
        0,
        None,
        Sha256Digest::new([0; 32]),
        HarnessCommandKind::RegisterGenesis { revision: revision.clone() },
    )
    .unwrap();
    let genesis_transition = decide(None, &genesis).unwrap();
    let store_id = StoreId::new(fixtures_support::bytes(23)).unwrap();
    let mut journal =
        SqliteJournal::open(&database, store_id, SqliteJournalOptions::default()).unwrap();
    commit_harness_transition(&mut journal, &genesis, &genesis_transition).unwrap();

    let plan = MaterializationPlan::build(
        fixtures_support::command_id(24),
        fixtures_support::event_id(25),
        &revision,
        ObservedTarget::new(
            WorkspaceSnapshot::from_c1(&fixtures_support::workspace_snapshot()),
            Vec::new(),
        )
        .unwrap(),
        MaterializationReason::Forward,
        None,
    )
    .unwrap();
    let plan_id = plan.id();
    let plan_command = HarnessCommand::new(
        fixtures_support::command_id(24),
        fixtures_support::event_id(25),
        revision.harness_id(),
        genesis_transition.state().sequence(),
        Some(genesis_transition.state().last_event_id()),
        genesis_transition.state().state_digest(),
        HarnessCommandKind::PlanMaterialization { plan: plan.clone() },
    )
    .unwrap();
    let committed = HarnessRuntime::new(&mut journal, &artifact_store, &artifact_store)
        .commit_plan(genesis_transition.state(), &plan_command)
        .unwrap();
    let PlanningOutcome::Committed(committed) = committed else {
        panic!("fresh plan must commit");
    };
    let planned_state = committed.state().clone();

    let replay = load_harness_replay(&journal, revision.harness_id()).unwrap();
    let runtime = HarnessRuntime::new(&mut journal, &artifact_store, &artifact_store);
    let recovered = runtime.recover_plan(&replay, plan_id).unwrap();
    assert_eq!(recovered.plan().id(), plan_id);
    assert_eq!(recovered.evidence(), &PlanCommitEvidence::Recovered { store_id },);

    let claimed = journal.claim_outbox(10, 20).unwrap().expect("committed plan directive");
    let claim = DirectiveClaim::from_message(&plan, &claimed).unwrap();
    let settlement_event = fixtures_support::event_id(27);
    let failure = MaterializationFailure::new(
        plan_id,
        plan.digest(),
        MaterializationFailureCode::StaleWorkspace,
        peritus_codec::sha256(b"workspace changed before execution"),
        21,
        settlement_event,
    );
    let settlement = HarnessCommand::new(
        fixtures_support::command_id(26),
        settlement_event,
        revision.harness_id(),
        planned_state.sequence(),
        Some(planned_state.last_event_id()),
        planned_state.state_digest(),
        HarnessCommandKind::RecordMaterializationFailure { failure },
    )
    .unwrap();
    let settlement_transition = decide(Some(&planned_state), &settlement).unwrap();
    commit_harness_settlement(&mut journal, &settlement, &settlement_transition, claim).unwrap();
    assert!(journal.claim_outbox(30, 40).unwrap().is_none());

    let settled_replay = load_harness_replay(&journal, revision.harness_id()).unwrap();
    assert_eq!(settled_replay.rebuild().unwrap(), Some(settlement_transition.state().clone()),);
}
