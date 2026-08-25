//! Real `SQLite` commit, exact resolution, restart, and corruption checks.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    SchedulerCommand, SchedulerCommandKind, SchedulerErrorKind, commit_scheduler_transition,
    load_scheduler_replay, start,
};
use peritus_types::{CommandId, EventId};

use support::{Fixture, bytes, digest};

#[test]
fn commit_restart_resolution_and_conflicting_command_are_exact() {
    let fixture = Fixture::new();
    let command = SchedulerCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        fixture.binding.run_id(),
        0,
        None,
        digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding.clone() },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.sqlite3");
    let store = StoreId::new(bytes(90)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let first = commit_scheduler_transition(&mut journal, &command, &transition).unwrap();
    let resolved = commit_scheduler_transition(&mut journal, &command, &transition).unwrap();
    assert_eq!(first.batch_hash(), resolved.batch_hash());
    drop(journal);
    let restarted = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let replay = load_scheduler_replay(&restarted, fixture.binding.run_id()).unwrap();
    assert_eq!(replay.rebuild().unwrap(), Some(transition.state().clone()));
    drop(restarted);

    let altered = SchedulerCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(3)).unwrap(),
        fixture.binding.run_id(),
        0,
        None,
        digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding },
    )
    .unwrap();
    let altered_transition = start(&altered).unwrap();
    let mut restarted = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let error =
        commit_scheduler_transition(&mut restarted, &altered, &altered_transition).unwrap_err();
    assert_eq!(error.kind(), SchedulerErrorKind::Journal);
}

#[test]
fn missing_checkpoint_fails_closed() {
    let fixture = Fixture::new();
    let command = SchedulerCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        fixture.binding.run_id(),
        0,
        None,
        digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding.clone() },
    )
    .unwrap();
    let transition = start(&command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scheduler.sqlite3");
    let store = StoreId::new(bytes(91)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    commit_scheduler_transition(&mut journal, &command, &transition).unwrap();
    drop(journal);
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .execute(
                "DELETE FROM state_records WHERE namespace = ?1",
                [i64::from(peritus_scheduler::SCHEDULER_STATE_NAMESPACE)]
            )
            .unwrap(),
        1
    );
    drop(connection);
    let journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    assert!(load_scheduler_replay(&journal, fixture.binding.run_id()).is_err());
}
