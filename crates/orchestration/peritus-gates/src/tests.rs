#![allow(clippy::unwrap_used, reason = "tests use fixed valid fixtures")]

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::GateId;

use crate::test_support as support;
use crate::wire::{GateCommandFrame, GateEventFrame, GateStateFrame};
use crate::{
    GateCommandKind, GateErrorKind, GateRejection, GateRunPhase, GateSlotPhase, GateTerminalKind,
    RecoveryDisposition,
};

mod action_identity;
mod evidence_binding;
mod pause;

#[test]
fn dependency_failure_blocks_successor_and_replays_exactly() {
    let fixture = support::fixture(2);
    assert_eq!(fixture.plan.execution_order(), [fixture.first, fixture.second]);
    assert!(crate::dependency_order_is_legal(&fixture.plan));
    let start = support::start_command(&fixture, 20);
    let started = crate::start(&fixture.plan, &start).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();

    let premature_attempt = support::attempt(&fixture, 31, 1);
    let premature = support::command(
        &state,
        21,
        GateCommandKind::PrepareAttempt { gate_id: fixture.second, attempt: premature_attempt },
    );
    let error = crate::decide(&fixture.plan, &state, &premature).expect_err("dependency");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::DependencyUnsatisfied));

    let first_attempt = support::attempt(&fixture, 32, 1);
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        22,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: first_attempt },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        23,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: first_attempt.execution_id(),
        },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        24,
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: first_attempt.execution_id(),
            result: support::candidate_failure(fixture.first, 40),
        },
    );
    assert_eq!(state.slot(fixture.first).unwrap().phase(), GateSlotPhase::Failed);
    assert_eq!(state.slot(fixture.second).unwrap().phase(), GateSlotPhase::Blocked);
    assert_eq!(state.slot(fixture.second).unwrap().blocked_by(), Some(fixture.first));

    advance_kind(&fixture.plan, &mut state, &mut events, 25, GateCommandKind::FinalizeRun);
    assert_eq!(state.terminal().unwrap().kind(), GateTerminalKind::Failed);
    assert!(crate::attempts_are_bounded(&fixture.plan, &state));
    assert!(crate::terminal_truthful(&state));
    assert!(crate::no_implicit_success(&state));
    let replayed = crate::replay(&fixture.plan, &events).expect("replay");
    assert!(crate::replay_equivalent(&state, &replayed));
    let mut reordered = events.clone();
    reordered.swap(1, 2);
    assert!(crate::replay(&fixture.plan, &reordered).is_err());
    let mut duplicated = events;
    duplicated.push(duplicated.last().expect("last event").clone());
    assert!(crate::replay(&fixture.plan, &duplicated).is_err());
}

#[test]
fn recovery_is_required_before_fresh_retry_and_cancellation_is_fail_closed() {
    let fixture = support::fixture(2);
    let start = support::start_command(&fixture, 50);
    let started = crate::start(&fixture.plan, &start).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    let first_attempt = support::attempt(&fixture, 51, 1);
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        51,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: first_attempt },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        52,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: first_attempt.execution_id(),
        },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        53,
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: first_attempt.execution_id(),
            result: support::retryable(fixture.first, 53),
        },
    );
    assert_eq!(state.slot(fixture.first).unwrap().phase(), GateSlotPhase::RecoveryPending);
    let fresh_attempt = support::attempt(&fixture, 54, 2);
    let early_retry = support::command(
        &state,
        54,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: fresh_attempt },
    );
    assert!(crate::decide(&fixture.plan, &state, &early_retry).is_err());

    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        55,
        GateCommandKind::ClassifyRecovery {
            gate_id: fixture.first,
            execution_id: first_attempt.execution_id(),
            disposition: RecoveryDisposition::SafeToRetry,
        },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        56,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: fresh_attempt },
    );
    advance_kind(&fixture.plan, &mut state, &mut events, 57, GateCommandKind::BeginCancellation);
    assert_eq!(state.phase(), GateRunPhase::Cancelling);
    assert_eq!(state.slot(fixture.first).unwrap().phase(), GateSlotPhase::Cancelled);
    assert_eq!(state.slot(fixture.second).unwrap().phase(), GateSlotPhase::Cancelled);
    advance_kind(&fixture.plan, &mut state, &mut events, 58, GateCommandKind::FinalizeRun);
    assert_eq!(state.terminal().unwrap().kind(), GateTerminalKind::Cancelled);
    assert_eq!(crate::replay(&fixture.plan, &events).expect("replay"), state);
}

#[test]
fn dispatched_attempt_without_a_result_recovers_as_indeterminate() {
    let fixture = support::fixture(1);
    let started =
        crate::start(&fixture.plan, &support::start_command(&fixture, 60)).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    let attempt = support::attempt(&fixture, 61, 1);
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        61,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        62,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
        },
    );
    advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        63,
        GateCommandKind::ClassifyRecovery {
            gate_id: fixture.first,
            execution_id: attempt.execution_id(),
            disposition: RecoveryDisposition::TerminalFailure,
        },
    );
    advance_kind(&fixture.plan, &mut state, &mut events, 64, GateCommandKind::FinalizeRun);
    assert_eq!(state.terminal().unwrap().kind(), GateTerminalKind::Indeterminate);
    assert_eq!(crate::replay(&fixture.plan, &events).expect("replay"), state);
}

#[test]
fn complete_evidence_is_required_for_every_passing_dependency() {
    let fixture = support::fixture(1);
    let start = support::start_command(&fixture, 70);
    let started = crate::start(&fixture.plan, &start).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    pass_gate(&fixture, fixture.first, 71, &mut state, &mut events);
    assert!(crate::dependencies_are_satisfied(&fixture.plan, &state, fixture.second));
    pass_gate(&fixture, fixture.second, 80, &mut state, &mut events);
    advance_kind(&fixture.plan, &mut state, &mut events, 89, GateCommandKind::FinalizeRun);
    assert_eq!(state.terminal().unwrap().kind(), GateTerminalKind::Passed);
    assert!(crate::terminal_truthful(&state));
    assert!(crate::no_implicit_success(&state));
    assert_eq!(crate::replay(&fixture.plan, &events).expect("replay"), state);
}

#[test]
fn all_d1_families_round_trip_and_checkpoint_matches_replay() {
    let fixture = support::fixture(1);
    let command = support::start_command(&fixture, 100);
    let transition = crate::start(&fixture.plan, &command).expect("start");

    let command_bytes =
        encode_message(&GateCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .expect("encode command");
    let decoded_command =
        decode_message::<GateCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .expect("decode command")
            .into_command();
    assert_eq!(decoded_command, command);

    let event_bytes =
        encode_message(&GateEventFrame(transition.event().clone()), CodecLimits::PRODUCTION)
            .expect("encode event");
    let decoded_event = decode_message::<GateEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
        .expect("decode event")
        .into_event();
    assert_eq!(&decoded_event, transition.event());

    let mut events = vec![transition.event().clone()];
    let mut state = transition.state().clone();
    pass_gate(&fixture, fixture.first, 101, &mut state, &mut events);
    let attempt = state.slot(fixture.first).unwrap().active_attempt().unwrap();
    assert_eq!(state.used_executions(), [attempt.execution_id()]);
    assert_eq!(state.used_actions(), [attempt.action_id()]);
    let frame = GateStateFrame::from_state(&state);
    let state_bytes = encode_message(&frame, CodecLimits::PRODUCTION).expect("encode state");
    let decoded_state = decode_message::<GateStateFrame>(&state_bytes, CodecLimits::PRODUCTION)
        .expect("decode state");
    assert!(decoded_state.matches_state(&state));
    let replayed = crate::replay(&fixture.plan, &events).expect("replay receipt state");
    assert_eq!(replayed, state);
}

#[test]
fn atomic_genesis_commit_is_idempotent_and_restart_checked() {
    let fixture = support::fixture(1);
    let command = support::start_command(&fixture, 110);
    let transition = crate::start(&fixture.plan, &command).expect("start");
    let temporary = tempfile::tempdir().expect("temporary journal");
    let store_id = StoreId::new([111; 16]).expect("store id");
    let mut journal = SqliteJournal::open(
        temporary.path().join("gate.sqlite3"),
        store_id,
        SqliteJournalOptions::default(),
    )
    .expect("open journal");
    let first =
        crate::commit_gate_transition(&mut journal, &command, &transition).expect("first commit");
    let resolved = crate::commit_gate_transition(&mut journal, &command, &transition)
        .expect("idempotent resolution");
    assert_eq!(first.batch_hash(), resolved.batch_hash());
    let replay = crate::load_gate_replay(&journal, fixture.run_id).expect("load replay");
    assert_eq!(replay.rebuild(&fixture.plan).expect("rebuild"), Some(transition.state().clone()));
}

fn pass_gate(
    fixture: &support::Fixture,
    gate_id: GateId,
    seed: u8,
    state: &mut crate::GateRunState,
    events: &mut Vec<crate::GateEvent>,
) {
    let attempt = support::attempt(fixture, seed, 1);
    advance_kind(
        &fixture.plan,
        state,
        events,
        seed,
        GateCommandKind::PrepareAttempt { gate_id, attempt },
    );
    advance_kind(
        &fixture.plan,
        state,
        events,
        seed.wrapping_add(1),
        GateCommandKind::MarkDispatched { gate_id, execution_id: attempt.execution_id() },
    );
    advance_kind(
        &fixture.plan,
        state,
        events,
        seed.wrapping_add(2),
        GateCommandKind::ObserveResult {
            gate_id,
            execution_id: attempt.execution_id(),
            result: support::passing(gate_id, seed.wrapping_add(20)),
        },
    );
    let receipt = support::empty_receipt(fixture, state, gate_id, seed);
    advance_kind(
        &fixture.plan,
        state,
        events,
        seed.wrapping_add(3),
        GateCommandKind::PublishEvidence { gate_id, execution_id: attempt.execution_id(), receipt },
    );
}

fn advance(
    plan: &crate::GatePlan,
    state: &mut crate::GateRunState,
    events: &mut Vec<crate::GateEvent>,
    command: &crate::GateCommand,
) {
    let transition = crate::decide(plan, state, command).expect("accepted transition");
    events.push(transition.event().clone());
    *state = transition.into_state();
}

fn advance_kind(
    plan: &crate::GatePlan,
    state: &mut crate::GateRunState,
    events: &mut Vec<crate::GateEvent>,
    seed: u8,
    kind: GateCommandKind,
) {
    let command = support::command(state, seed, kind);
    advance(plan, state, events, &command);
}
