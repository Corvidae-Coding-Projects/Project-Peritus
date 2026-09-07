//! Exact session reuse, cold restart, competing writers and live corruption.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_codec::{CodecLimits, encode_message};
use peritus_journal::{CommandResolution, SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_scheduler::{
    SchedulerCommand, SchedulerCommandKind, SchedulerErrorKind, SchedulerSession, SchedulerState,
    SchedulerStateFrame, commit_scheduler_transition, decide, load_scheduler_replay, start,
};
use peritus_types::{CommandId, EventId};
use tempfile::TempDir;

use support::{Fixture, bytes, digest};

fn open(directory: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        directory.path().join("scheduler.sqlite3"),
        StoreId::new(bytes(90)).unwrap(),
        SqliteJournalOptions::default(),
    )
    .unwrap()
}

fn genesis(fixture: &Fixture) -> SchedulerCommand {
    SchedulerCommand::new(
        CommandId::new(bytes(1)).unwrap(),
        EventId::new(bytes(2)).unwrap(),
        fixture.binding.run_id(),
        0,
        None,
        digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding.clone() },
    )
    .unwrap()
}

fn started(directory: &TempDir) -> (SqliteJournal, SchedulerSession, SchedulerState) {
    let fixture = Fixture::new();
    let mut journal = open(directory);
    let mut session = SchedulerSession::default();
    let command = genesis(&fixture);
    assert!(session.state(&journal, command.run_id()).unwrap().is_none());
    let transition = start(&command).unwrap();
    let state = transition.state().clone();
    session.commit(&mut journal, &command, transition).unwrap();
    (journal, session, state)
}

#[test]
fn warm_successors_and_restarted_replay_preserve_every_canonical_byte() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, mut expected) = started(&directory);
    for identity in 3..35 {
        let prior = session.state(&journal, expected.run_id()).unwrap().unwrap();
        assert_eq!(prior, &expected);
        let kind = if identity % 2 == 1 {
            SchedulerCommandKind::PauseScheduler
        } else {
            SchedulerCommandKind::ResumeScheduler
        };
        let command = Fixture::command(prior, identity, kind);
        let transition = decide(prior, &command).unwrap();
        expected = transition.state().clone();
        let receipt = session.commit(&mut journal, &command, transition).unwrap();
        assert_eq!(receipt.records()[0].event_id(), command.event_id());
        let cold =
            load_scheduler_replay(&journal, expected.run_id()).unwrap().rebuild().unwrap().unwrap();
        assert_eq!(cold, expected);
        let canonical =
            encode_message(&SchedulerStateFrame::from_state(&expected), CodecLimits::PRODUCTION)
                .unwrap();
        let historical = journal
            .state_record_revision(
                peritus_scheduler::SCHEDULER_STATE_NAMESPACE,
                &peritus_scheduler::scheduler_state_key(expected.run_id()),
                expected.sequence().get(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(historical.bytes(), canonical);
    }
    drop(journal);
    let mut restarted = open(&directory);
    // Reusing the Rust session across a reopened connection must still perform cold replay.
    assert_eq!(session.state(&restarted, expected.run_id()).unwrap(), Some(&expected));
    assert_eq!(restarted.integrity_scan().unwrap().event_count(), 33);
}

#[test]
fn ordinary_same_owner_and_external_commits_invalidate_the_old_state() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, state) = started(&directory);
    let pause = Fixture::command(&state, 3, SchedulerCommandKind::PauseScheduler);
    let paused = decide(&state, &pause).unwrap();
    // This is also the E0 child path: its existing historical replay and plain commit remain.
    commit_scheduler_transition(&mut journal, &pause, &paused).unwrap();
    assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(paused.state()));
    let mut external = open(&directory);
    let resume = Fixture::command(paused.state(), 4, SchedulerCommandKind::ResumeScheduler);
    let resumed = decide(paused.state(), &resume).unwrap();
    commit_scheduler_transition(&mut external, &resume, &resumed).unwrap();
    assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(resumed.state()));
}

#[test]
fn racing_writer_rejects_planned_transition_and_then_replays_its_actual_successor() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, state) = started(&directory);
    let ours = Fixture::command(&state, 3, SchedulerCommandKind::PauseScheduler);
    let planned = decide(session.state(&journal, state.run_id()).unwrap().unwrap(), &ours).unwrap();
    let mut external = open(&directory);
    let theirs = Fixture::command(&state, 4, SchedulerCommandKind::PauseScheduler);
    let actual = decide(&state, &theirs).unwrap();
    commit_scheduler_transition(&mut external, &theirs, &actual).unwrap();
    assert!(session.commit(&mut journal, &ours, planned).is_err());
    assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(actual.state()));
    assert_eq!(journal.integrity_scan().unwrap().event_count(), 2);
}

#[test]
fn live_external_corruption_is_not_hidden_by_a_warm_session() {
    for sql in [
        "UPDATE events SET frame = zeroblob(length(frame)) WHERE sequence = 1",
        "UPDATE state_records SET value = zeroblob(length(value))",
        "DELETE FROM state_records",
        "UPDATE aggregate_heads SET sequence = sequence + 1",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (journal, mut session, state) = started(&directory);
        assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(&state));
        let external =
            rusqlite::Connection::open(directory.path().join("scheduler.sqlite3")).unwrap();
        external.execute_batch(sql).unwrap();
        assert!(session.state(&journal, state.run_id()).is_err(), "{sql}");
        assert!(session.state(&journal, state.run_id()).is_err(), "no stale fallback: {sql}");
    }
}

#[test]
fn corruption_after_session_planning_is_rejected_at_the_actual_append_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, state) = started(&directory);
    let pause = Fixture::command(&state, 3, SchedulerCommandKind::PauseScheduler);
    let planned =
        decide(session.state(&journal, state.run_id()).unwrap().unwrap(), &pause).unwrap();
    let external = rusqlite::Connection::open(directory.path().join("scheduler.sqlite3")).unwrap();
    external
        .execute("UPDATE events SET frame = zeroblob(length(frame)) WHERE sequence = 1", [])
        .unwrap();
    assert!(session.commit(&mut journal, &pause, planned).is_err());
    let frame = peritus_scheduler::SchedulerCommandFrame::from_command(&pause);
    let request = encode_message(&frame, CodecLimits::PRODUCTION).unwrap();
    assert!(matches!(
        journal.resolve_command(pause.command_id(), peritus_codec::sha256(&request)).unwrap(),
        CommandResolution::DefinitelyAbsent
    ));
    assert!(session.state(&journal, state.run_id()).is_err());
}

#[test]
fn switching_runs_and_failed_storage_cannot_publish_a_planned_successor() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, state) = started(&directory);
    let absent = peritus_types::RunId::new(bytes(80)).unwrap();
    assert!(session.state(&journal, absent).unwrap().is_none());
    assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(&state));
    let duplicate_event = SchedulerCommand::new(
        CommandId::new(bytes(3)).unwrap(),
        state.last_event_id(),
        state.run_id(),
        state.sequence().get(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.binding().revision(),
        SchedulerCommandKind::PauseScheduler,
    )
    .unwrap();
    let transition = decide(&state, &duplicate_event).unwrap();
    assert!(session.commit(&mut journal, &duplicate_event, transition).is_err());
    assert_eq!(session.state(&journal, state.run_id()).unwrap(), Some(&state));
    assert_eq!(journal.integrity_scan().unwrap().event_count(), 1);
}

#[test]
fn commit_without_matching_replay_and_invalid_pure_commands_do_not_advance_state() {
    let directory = tempfile::tempdir().unwrap();
    let (mut journal, mut session, state) = started(&directory);
    let pause = Fixture::command(&state, 3, SchedulerCommandKind::PauseScheduler);
    let transition = decide(&state, &pause).unwrap();
    let mut empty = SchedulerSession::default();
    assert!(empty.commit(&mut journal, &pause, transition.clone()).is_err());
    let invalid = Fixture::command(&state, 4, SchedulerCommandKind::ResumeScheduler);
    assert_eq!(
        decide(session.state(&journal, state.run_id()).unwrap().unwrap(), &invalid)
            .unwrap_err()
            .kind(),
        SchedulerErrorKind::IllegalTransition
    );
    session.commit(&mut journal, &pause, transition).unwrap();
    let prior = session.state(&journal, state.run_id()).unwrap().unwrap();
    assert!(decide(prior, &pause).is_err());
    assert_eq!(journal.integrity_scan().unwrap().event_count(), 2);
}
