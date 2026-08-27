//! Pause/resume reduction, canonical wire, and real-journal restart coverage.

#![allow(clippy::unwrap_used, reason = "tests use fixed valid fixtures")]

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use crate::test_support as support;
use crate::wire::{GateCommandFrame, GateEventFrame};
use crate::{
    GateCommandKind, GateErrorKind, GateEvent, GateEventKind, GateRejection, GateResumePhase,
    GateRunPhase, GateTerminalKind, RecoveryDisposition,
};

#[test]
fn active_pause_is_canonical_durable_idempotent_and_restart_safe() {
    let fixture = support::fixture(2);
    let start_command = support::start_command(&fixture, 120);
    let started = crate::start(&fixture.plan, &start_command).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gate-pause.sqlite3");
    let store_id = StoreId::new([121; 16]).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    crate::commit_gate_transition(&mut journal, &start_command, &started).unwrap();
    let active = started.into_state();

    let pause_command = support::command(&active, 122, GateCommandKind::PauseRun);
    let paused = crate::decide(&fixture.plan, &active, &pause_command).unwrap();
    assert_eq!(paused.state().phase(), GateRunPhase::Paused(GateResumePhase::Active));
    assert!(matches!(
        paused.event().kind(),
        GateEventKind::RunPaused { resume_phase: GateResumePhase::Active }
    ));
    assert_eq!(paused.state().slots(), active.slots());
    assert_eq!(paused.state().maximum_attempts(), active.maximum_attempts());
    assert!(paused.state().terminal().is_none());

    let command_bytes =
        encode_message(&GateCommandFrame::from_command(&pause_command), CodecLimits::PRODUCTION)
            .unwrap();
    assert_eq!(
        decode_message::<GateCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        pause_command
    );
    let event_bytes =
        encode_message(&GateEventFrame(paused.event().clone()), CodecLimits::PRODUCTION).unwrap();
    assert_eq!(
        decode_message::<GateEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        paused.event().clone()
    );

    let blocked = support::command(
        paused.state(),
        123,
        GateCommandKind::PrepareAttempt {
            gate_id: fixture.first,
            attempt: support::attempt(&fixture, 124, 1),
        },
    );
    assert_eq!(
        crate::decide(&fixture.plan, paused.state(), &blocked).unwrap_err().kind(),
        GateErrorKind::Rejected(GateRejection::IllegalTransition)
    );

    let first = crate::commit_gate_lifecycle_transition(&mut journal, &pause_command).unwrap();
    let resolved = crate::commit_gate_lifecycle_transition(&mut journal, &pause_command).unwrap();
    assert_eq!(first.batch_hash(), resolved.batch_hash());
    let stale_pause = support::command(&active, 126, GateCommandKind::PauseRun);
    let stale_error = crate::commit_gate_lifecycle_transition(&mut journal, &stale_pause)
        .expect_err("a different command at the reconciled child head must fail");
    assert_eq!(stale_error.kind(), GateErrorKind::Journal);
    let expected_paused = paused.into_state();
    drop(journal);

    let mut restarted =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let replay = crate::load_gate_replay(&restarted, fixture.run_id).unwrap();
    let replayed_paused = replay.rebuild(&fixture.plan).unwrap().unwrap();
    assert_eq!(replayed_paused, expected_paused);

    let resume_command = support::command(&replayed_paused, 125, GateCommandKind::ResumeRun);
    let resumed = crate::decide(&fixture.plan, &replayed_paused, &resume_command).unwrap();
    assert_eq!(resumed.state().phase(), GateRunPhase::Active);
    assert_eq!(resumed.state().slots(), active.slots());
    assert_eq!(resumed.state().maximum_attempts(), active.maximum_attempts());
    assert!(resumed.state().terminal().is_none());
    crate::commit_gate_lifecycle_transition(&mut restarted, &resume_command).unwrap();
    crate::commit_gate_lifecycle_transition(&mut restarted, &resume_command).unwrap();
    let expected_resumed = resumed.into_state();
    drop(restarted);

    let restarted = SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    let replay = crate::load_gate_replay(&restarted, fixture.run_id).unwrap();
    assert_eq!(replay.rebuild(&fixture.plan).unwrap(), Some(expected_resumed));
}

#[test]
fn plan_free_commit_rejects_every_non_lifecycle_command() {
    let fixture = support::fixture(2);
    let start_command = support::start_command(&fixture, 140);
    let started = crate::start(&fixture.plan, &start_command).unwrap();
    let active = started.state().clone();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gate-lifecycle-closed.sqlite3");
    let store_id = StoreId::new([141; 16]).unwrap();
    let mut journal =
        SqliteJournal::open(&path, store_id, SqliteJournalOptions::default()).unwrap();
    crate::commit_gate_transition(&mut journal, &start_command, &started).unwrap();

    let attempt = support::attempt(&fixture, 142, 1);
    let mut evidence_state = active.clone();
    let mut evidence_events = Vec::new();
    advance(
        &fixture.plan,
        &mut evidence_state,
        &mut evidence_events,
        143,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    advance(
        &fixture.plan,
        &mut evidence_state,
        &mut evidence_events,
        144,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    advance(
        &fixture.plan,
        &mut evidence_state,
        &mut evidence_events,
        145,
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            result: support::passing(fixture.first, 146),
        },
    );
    let receipt = support::empty_receipt(&fixture, &evidence_state, fixture.first, 147);
    let kinds = vec![
        GateCommandKind::StartRun { snapshot_digest: fixture.snapshot },
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            result: support::passing(fixture.first, 148),
        },
        GateCommandKind::ClassifyRecovery {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            disposition: RecoveryDisposition::SafeToRetry,
        },
        GateCommandKind::PublishEvidence {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            receipt,
        },
        GateCommandKind::BeginCancellation,
        GateCommandKind::FinalizeRun,
    ];
    for (offset, kind) in kinds.into_iter().enumerate() {
        let seed = 149_u8.wrapping_add(u8::try_from(offset).unwrap());
        let command = support::command(&active, seed, kind);
        let error = crate::commit_gate_lifecycle_transition(&mut journal, &command)
            .expect_err("plan-free durability must reject non-lifecycle commands");
        assert_eq!(error.kind(), GateErrorKind::Journal);
    }
}

#[test]
fn cancelling_pause_resumes_cancelling_and_cannot_finalize_while_paused() {
    let fixture = support::fixture(1);
    let started = crate::start(&fixture.plan, &support::start_command(&fixture, 130)).unwrap();
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    advance(&fixture.plan, &mut state, &mut events, 131, GateCommandKind::BeginCancellation);
    advance(&fixture.plan, &mut state, &mut events, 132, GateCommandKind::PauseRun);
    assert_eq!(state.phase(), GateRunPhase::Paused(GateResumePhase::Cancelling));

    let finalize = support::command(&state, 133, GateCommandKind::FinalizeRun);
    assert_eq!(
        crate::decide(&fixture.plan, &state, &finalize).unwrap_err().kind(),
        GateErrorKind::Rejected(GateRejection::IllegalTransition)
    );
    advance(&fixture.plan, &mut state, &mut events, 134, GateCommandKind::ResumeRun);
    assert_eq!(state.phase(), GateRunPhase::Cancelling);
    advance(&fixture.plan, &mut state, &mut events, 135, GateCommandKind::FinalizeRun);
    assert_eq!(state.terminal().unwrap().kind(), GateTerminalKind::Cancelled);
    assert_eq!(crate::replay(&fixture.plan, &events).unwrap(), state);
}

fn advance(
    plan: &crate::GatePlan,
    state: &mut crate::GateRunState,
    events: &mut Vec<GateEvent>,
    seed: u8,
    kind: GateCommandKind,
) {
    let transition = crate::decide(plan, state, &support::command(state, seed, kind)).unwrap();
    events.push(transition.event().clone());
    *state = transition.into_state();
}
