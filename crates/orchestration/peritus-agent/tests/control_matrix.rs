//! Pause/resume and cancellation coverage for every active D0 phase.

mod common;

use common::*;
use peritus_agent::{
    ActivePhase, AgentCommandKind, AgentEvent, AgentPhase, AgentTurnState, ModelCallId,
    TerminalKind, ToolOrdinal, ToolResultStatus, ToolSideEffect, replay,
};

#[test]
fn every_active_phase_pauses_resumes_and_cancels_explicitly() {
    for (phase, events, state) in active_phase_prefixes() {
        let mut paused_events = events.clone();
        let mut paused = state.clone();
        apply(&mut paused, &mut paused_events, AgentCommandKind::Paused);
        assert_eq!(paused.phase(), AgentPhase::Paused, "pause from {phase:?}");
        assert_eq!(paused.paused_from(), Some(phase));
        apply(
            &mut paused,
            &mut paused_events,
            AgentCommandKind::Resumed { recovery_checked: true },
        );
        assert_eq!(paused.phase(), AgentPhase::Active(phase), "resume to {phase:?}");
        assert_eq!(replay(&paused_events).expect("paused replay"), paused);

        let mut cancelled_events = events;
        let mut cancelled = state;
        apply(&mut cancelled, &mut cancelled_events, AgentCommandKind::CancellationRequested);
        assert_eq!(cancelled.phase(), AgentPhase::Cancelling, "cancel from {phase:?}");
        apply(&mut cancelled, &mut cancelled_events, AgentCommandKind::CancellationFinished);
        assert_eq!(cancelled.terminal_kind(), Some(TerminalKind::Cancelled));
        assert_eq!(replay(&cancelled_events).expect("cancel replay"), cancelled);
    }
}

fn active_phase_prefixes() -> Vec<(ActivePhase, Vec<AgentEvent>, AgentTurnState)> {
    let (mut events, mut state) = started(64);
    let mut prefixes = vec![(ActivePhase::PreparingContext, events.clone(), state.clone())];
    apply(&mut state, &mut events, AgentCommandKind::ContextPrepared(context()));
    prefixes.push((ActivePhase::RequestingModel, events.clone(), state.clone()));
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(digest(24)).expect("call"),
            request_digest: digest(25),
        },
    );
    prefixes.push((ActivePhase::StreamingResponse, events.clone(), state.clone()));
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(0, ToolSideEffect::None)],
        },
    );
    prefixes.push((ActivePhase::ProposedToolCalls, events.clone(), state.clone()));
    apply(&mut state, &mut events, AgentCommandKind::AuthorizationStarted);
    prefixes.push((ActivePhase::AwaitingAuthorization, events.clone(), state.clone()));
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolAuthorized {
            ordinal: ToolOrdinal::new(0),
            authority_digest: digest(60),
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::ToolExecutionStarted);
    prefixes.push((ActivePhase::ExecutingTools, events.clone(), state.clone()));
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolDispatched { ordinal: ToolOrdinal::new(0) },
    );
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCompleted {
            ordinal: ToolOrdinal::new(0),
            result: result(ToolResultStatus::Succeeded, 70),
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::ResultRecordingStarted);
    prefixes.push((ActivePhase::RecordingResults, events, state));

    let (mut completion_events, mut completion_state) = started(64);
    start_model(&mut completion_state, &mut completion_events);
    apply(
        &mut completion_state,
        &mut completion_events,
        AgentCommandKind::CompletionProposed { terminal: terminal(), proposal: proposal() },
    );
    prefixes.push((ActivePhase::ProposedCompletion, completion_events, completion_state));
    prefixes
}
