//! Replay, pause, cancellation, and completion truthfulness tests.

mod common;

use common::*;
use peritus_agent::{
    ActivePhase, AgentCommand, AgentCommandKind, AgentErrorCode, AgentEvent, AgentPhase,
    ModelCallId, ModelTerminalRecord, ProviderEventRecord, ProviderRetryClass, ProviderRetryRecord,
    TerminalKind, reduce, replay, start,
};
use peritus_codec::CodecLimits;

#[test]
fn completion_requires_explicit_proposal_then_commit_and_replays_exactly() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::CompletionProposed { terminal: terminal(), proposal: proposal() },
    );
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::ProposedCompletion));
    assert!(state.completion().is_some());
    assert_eq!(state.terminal_kind(), None);

    apply(&mut state, &mut events, AgentCommandKind::CompletionCommitted);
    assert_eq!(state.terminal_kind(), Some(TerminalKind::Completed));

    let replayed = replay(&events).expect("replay");
    assert_eq!(replayed, state);
    assert_eq!(replayed.canonical_bytes(), state.canonical_bytes());
}

#[test]
fn partial_model_terminal_cannot_become_success_and_rejection_is_non_mutating() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    let before = state.clone();
    let offset = u8::try_from(state.sequence().get()).expect("sequence");
    let command = AgentCommand::new(
        id16(30 + offset, peritus_types::CommandId::new),
        id16(90 + offset, peritus_types::EventId::new),
        state.logical_revision(),
        state.state_digest(),
        AgentCommandKind::CompletionProposed {
            terminal: ModelTerminalRecord::new(digest(26), false, true, false),
            proposal: proposal(),
        },
    );
    let rejection = reduce(&state, &command).expect_err("partial response rejected");
    assert_eq!(rejection.code(), AgentErrorCode::CompletionIneligible);
    assert_eq!(state, before);
}

#[test]
fn pause_resume_and_cancel_are_explicit() {
    let (mut events, mut state) = started(64);
    apply(&mut state, &mut events, AgentCommandKind::Paused);
    assert_eq!(state.paused_from(), Some(ActivePhase::PreparingContext));
    apply(&mut state, &mut events, AgentCommandKind::Resumed { recovery_checked: true });
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::PreparingContext));
    apply(&mut state, &mut events, AgentCommandKind::CancellationRequested);
    assert_eq!(state.phase(), AgentPhase::Cancelling);
    apply(&mut state, &mut events, AgentCommandKind::CancellationFinished);
    assert_eq!(state.terminal_kind(), Some(TerminalKind::Cancelled));
    assert_eq!(replay(&events).expect("replay"), state);
}

#[test]
fn provider_retry_closes_in_flight_work_and_replays_exact_resume() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ProviderEventObserved(ProviderEventRecord::new(1, digest(44), 12, false)),
    );
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
            digest(45),
            digest(46),
            ProviderRetryClass::ExactResume { cursor: 1 },
        )),
    );
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::RequestingModel));
    assert!(!state.model().in_flight());
    assert_eq!(state.model().retry_count(), 1);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(digest(47)).expect("retry call"),
            request_digest: digest(46),
        },
    );
    assert_eq!(state.model().cursor(), 1);
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::StreamingResponse));
    assert_eq!(replay(&events).expect("replay"), state);
}

#[test]
fn protocol_records_recover_pure_retry_events_and_replay_exactly() {
    let configured_limits = limits(64);
    let started = start(
        binding(),
        configured_limits,
        id16(111, peritus_types::CommandId::new),
        id16(112, peritus_types::EventId::new),
    )
    .expect("start");
    let (_, genesis, _) =
        started.to_protocol_records(None, CodecLimits::PRODUCTION).expect("genesis records");
    let mut records = vec![genesis];
    let (_, mut state) = started.into_parts();

    for kind in [
        AgentCommandKind::ContextPrepared(context()),
        AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(digest(91)).expect("call"),
            request_digest: digest(92),
        },
        AgentCommandKind::ProviderEventObserved(ProviderEventRecord::new(1, digest(93), 7, false)),
        AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
            digest(94),
            digest(95),
            ProviderRetryClass::ExactResume { cursor: 1 },
        )),
    ] {
        let offset = u8::try_from(state.sequence().get()).expect("sequence");
        let command = AgentCommand::new(
            id16(120 + offset, peritus_types::CommandId::new),
            id16(130 + offset, peritus_types::EventId::new),
            state.logical_revision(),
            state.state_digest(),
            kind,
        );
        let transition = reduce(&state, &command).expect("reduce");
        let (_, event, _) = transition
            .to_protocol_records(Some(&command), CodecLimits::PRODUCTION)
            .expect("records");
        records.push(event);
        let (_, successor) = transition.into_parts();
        state = successor;
    }

    let recovered = AgentEvent::recover_protocol_events(&records, binding(), configured_limits)
        .expect("recover records");
    let replayed = replay(&recovered).expect("replay recovered events");
    assert_eq!(replayed, state);
}
