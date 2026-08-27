//! Real-SQLite atomic commit, idempotency, restart, conflict, and checkpoint tests.

#![allow(clippy::unwrap_used, reason = "fixed checked durability fixtures")]

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use crate::{
    ClaimedDirectiveAcknowledgement, DirectiveDestination, DirectiveKind,
    ORCHESTRATOR_STATE_NAMESPACE, OrchestratorCommandKind, OrchestratorErrorKind,
    commit_claimed_directive_acknowledgement, commit_orchestrator_transition,
    load_orchestrator_replay,
};

use super::support::{Scenario, bytes, handoff_payload, happy_path, publish};

#[test]
fn every_lifecycle_commit_restarts_to_the_exact_checkpoint() {
    let scenario = happy_path();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator.sqlite3");
    let store = StoreId::new(bytes(800)).unwrap();
    let run_id = scenario.state().binding().run_id();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();

    for (command, transition) in scenario.steps() {
        let committed = commit_orchestrator_transition(&mut journal, command, transition).unwrap();
        let resolved = commit_orchestrator_transition(&mut journal, command, transition).unwrap();
        assert_eq!(committed.batch_hash(), resolved.batch_hash());
        drop(journal);

        journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
        let recovered = load_orchestrator_replay(&journal, run_id)
            .unwrap()
            .rebuild()
            .unwrap()
            .expect("committed checkpoint");
        assert_eq!(&recovered, transition.state());
    }
}

#[test]
fn conflicting_command_identity_and_missing_checkpoint_fail_closed() {
    let scenario = Scenario::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator-conflict.sqlite3");
    let store = StoreId::new(bytes(801)).unwrap();
    let (command, transition) = &scenario.steps()[0];
    let run_id = command.run_id();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    commit_orchestrator_transition(&mut journal, command, transition).unwrap();

    let genesis = match command.kind() {
        OrchestratorCommandKind::Start { genesis } => Some(genesis.clone()),
        _ => None,
    }
    .expect("first fixture command is genesis");
    let conflicting_command = crate::OrchestratorCommand::new(
        command.command_id(),
        peritus_types::EventId::new(bytes(802)).unwrap(),
        command.run_id(),
        0,
        None,
        peritus_types::Sha256Digest::new([0; 32]),
        command.revision(),
        OrchestratorCommandKind::Start { genesis },
    )
    .unwrap();
    let conflicting_transition = crate::start(&conflicting_command).unwrap();
    let error =
        commit_orchestrator_transition(&mut journal, &conflicting_command, &conflicting_transition)
            .expect_err("one command identity cannot name different canonical bytes");
    assert_eq!(error.kind(), OrchestratorErrorKind::Conflict);
    drop(journal);

    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .execute(
                "DELETE FROM state_records WHERE namespace = ?1",
                [i64::from(ORCHESTRATOR_STATE_NAMESPACE)],
            )
            .unwrap(),
        1
    );
    drop(connection);
    let journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    assert!(load_orchestrator_replay(&journal, run_id).is_err());
}

#[test]
fn claimed_directive_acknowledgement_commits_with_outbox_settlement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator-claimed-ack.sqlite3");
    let store = StoreId::new(bytes(803)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let (scenario, claim, _) = claimed_acknowledgement(&mut journal, 0);
    let (command, transition) = scenario.steps().last().unwrap();

    commit_claimed_directive_acknowledgement(&mut journal, claim, command, transition).unwrap();

    assert!(journal.claim_outbox(30, 40).unwrap().is_none());
    let replay = load_orchestrator_replay(&journal, command.run_id()).unwrap();
    assert_eq!(replay.events().len(), 3);
    assert_eq!(replay.rebuild().unwrap().as_ref(), Some(transition.state()));
}

#[test]
fn stale_claim_fence_rolls_back_the_whole_e0_append() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator-stale-ack.sqlite3");
    let store = StoreId::new(bytes(804)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let (scenario, stale_claim, actual_fence) = claimed_acknowledgement(&mut journal, 1);
    let (command, transition) = scenario.steps().last().unwrap();

    commit_claimed_directive_acknowledgement(&mut journal, stale_claim, command, transition)
        .expect_err("stale claim fence must reject the entire append");

    let replay = load_orchestrator_replay(&journal, command.run_id()).unwrap();
    assert_eq!(replay.events().len(), 2);
    assert_eq!(replay.rebuild().unwrap().as_ref(), Some(scenario.steps()[1].1.state()));

    let publish_command = &scenario.steps()[1].0;
    let exact_claim = ClaimedDirectiveAcknowledgement::new(
        publish_command,
        stale_claim.outbox_id(),
        actual_fence,
    )
    .unwrap();
    commit_claimed_directive_acknowledgement(&mut journal, exact_claim, command, transition)
        .unwrap();
}

#[test]
fn claimed_acknowledgement_retry_resolves_the_exact_committed_batch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator-ack-retry.sqlite3");
    let store = StoreId::new(bytes(805)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let (scenario, claim, _) = claimed_acknowledgement(&mut journal, 0);
    let (command, transition) = scenario.steps().last().unwrap();

    let committed =
        commit_claimed_directive_acknowledgement(&mut journal, claim, command, transition).unwrap();
    let resolved =
        commit_claimed_directive_acknowledgement(&mut journal, claim, command, transition).unwrap();

    assert_eq!(committed.batch_hash(), resolved.batch_hash());
    assert_eq!(committed.records(), resolved.records());
    let replay = load_orchestrator_replay(&journal, command.run_id()).unwrap();
    assert_eq!(replay.events().len(), 3);
    assert!(journal.claim_outbox(30, 40).unwrap().is_none());
}

fn claimed_acknowledgement(
    journal: &mut SqliteJournal,
    fence_offset: u64,
) -> (Scenario, ClaimedDirectiveAcknowledgement, u64) {
    let mut scenario = Scenario::new();
    let (genesis_command, genesis_transition) = &scenario.steps()[0];
    commit_orchestrator_transition(journal, genesis_command, genesis_transition).unwrap();

    let handoff = scenario.state().open_handoff().unwrap().clone();
    let directive_id = publish(
        &mut scenario,
        DirectiveDestination::Collaboration,
        DirectiveKind::StartWriter,
        handoff_payload(&handoff, DirectiveKind::StartWriter),
        Some(handoff.task_id()),
        Some(handoff.work_id()),
        None,
    );
    let (publish_command, publish_transition) = scenario.steps().last().unwrap();
    commit_orchestrator_transition(journal, publish_command, publish_transition).unwrap();
    let message = journal.claim_outbox(10, 20).unwrap().expect("published E0 directive");
    assert_eq!(message.id().as_bytes(), directive_id.as_bytes());
    let actual_fence = message.fence().expect("claimed outbox fence");
    let supplied_fence = actual_fence.checked_add(fence_offset).unwrap();
    let claim = ClaimedDirectiveAcknowledgement::new(publish_command, message.id(), supplied_fence)
        .unwrap();
    scenario.apply_ok(OrchestratorCommandKind::AcknowledgeDirective { directive_id });
    (scenario, claim, actual_fence)
}
