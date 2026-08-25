//! Real-SQLite atomic commit, idempotency, restart, conflict, and checkpoint tests.

#![allow(clippy::unwrap_used, reason = "fixed checked durability fixtures")]

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use crate::{
    ORCHESTRATOR_STATE_NAMESPACE, OrchestratorErrorKind, commit_orchestrator_transition,
    load_orchestrator_replay,
};

use super::support::{Scenario, bytes, happy_path};

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
        crate::OrchestratorCommandKind::Start { genesis } => Some(genesis.clone()),
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
        crate::OrchestratorCommandKind::Start { genesis },
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
