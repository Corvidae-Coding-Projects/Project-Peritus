//! Real-SQLite atomic commit, restart, idempotency, and command-conflict coverage.

#![allow(clippy::unwrap_used, reason = "fixed durability fixtures use checked values")]

mod support;

use peritus_collaboration::{
    CollaborationCommand, CollaborationCommandKind, CollaborationErrorKind,
    commit_collaboration_transition, load_collaboration_replay, start,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::{CommandId, EventId};

use support::{Fixture, bytes, digest};

#[test]
fn exact_transition_survives_restart_and_resolves_idempotently() {
    let fixture = Fixture::new(peritus_collaboration::JoinPolicy::NoChildren);
    let command = CollaborationCommand::new(
        CommandId::new(bytes(70)).unwrap(),
        EventId::new(bytes(71)).unwrap(),
        fixture.run_id,
        0,
        None,
        digest(0),
        fixture.revision,
        CollaborationCommandKind::Start { binding: fixture.binding.clone() },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collaboration.sqlite3");
    let store_id = StoreId::new(bytes(72)).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let first = commit_collaboration_transition(&mut journal, &command, &transition).unwrap();
    let repeated = commit_collaboration_transition(&mut journal, &command, &transition).unwrap();
    assert_eq!(first.batch_hash(), repeated.batch_hash());
    drop(journal);
    let restarted = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let replay = load_collaboration_replay(&restarted, fixture.run_id).unwrap();
    assert_eq!(replay.rebuild().unwrap(), Some(transition.state().clone()));
}

#[test]
fn reused_command_identity_with_different_bytes_is_a_conflict() {
    let fixture = Fixture::new(peritus_collaboration::JoinPolicy::NoChildren);
    let first = CollaborationCommand::new(
        CommandId::new(bytes(80)).unwrap(),
        EventId::new(bytes(81)).unwrap(),
        fixture.run_id,
        0,
        None,
        digest(0),
        fixture.revision,
        CollaborationCommandKind::Start { binding: fixture.binding.clone() },
    )
    .unwrap();
    let first_transition = start(&first).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut journal = SqliteJournal::open(
        directory.path().join("conflict.sqlite3"),
        StoreId::new(bytes(82)).unwrap(),
        SqliteJournalOptions::default(),
    )
    .unwrap();
    commit_collaboration_transition(&mut journal, &first, &first_transition).unwrap();
    let conflicting = CollaborationCommand::new(
        first.command_id(),
        EventId::new(bytes(83)).unwrap(),
        fixture.run_id,
        0,
        None,
        digest(0),
        fixture.revision,
        CollaborationCommandKind::Start { binding: fixture.binding },
    )
    .unwrap();
    let conflicting_transition = start(&conflicting).unwrap();
    let error =
        commit_collaboration_transition(&mut journal, &conflicting, &conflicting_transition)
            .expect_err("same command identity cannot name different canonical bytes");
    assert_eq!(error.kind(), CollaborationErrorKind::Journal);
}
