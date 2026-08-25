//! Ordered tool-call lifecycle and mutation-serialization tests.

mod common;

use common::*;
use peritus_agent::{
    ActivePhase, AgentCommand, AgentCommandKind, AgentErrorCode, AgentPhase, ToolOrdinal,
    ToolResultStatus, ToolSideEffect, reduce, replay,
};

#[test]
fn parallel_terminal_arrival_retains_proposal_order() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(0, ToolSideEffect::None), tool(1, ToolSideEffect::Process)],
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::AuthorizationStarted);
    for ordinal in [0, 1] {
        apply(
            &mut state,
            &mut events,
            AgentCommandKind::ToolAuthorized {
                ordinal: ToolOrdinal::new(ordinal),
                authority_digest: digest(60 + u8::try_from(ordinal).expect("test ordinal")),
            },
        );
    }
    apply(&mut state, &mut events, AgentCommandKind::ToolExecutionStarted);
    for ordinal in [0, 1] {
        apply(
            &mut state,
            &mut events,
            AgentCommandKind::ToolDispatched { ordinal: ToolOrdinal::new(ordinal) },
        );
    }
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCompleted {
            ordinal: ToolOrdinal::new(1),
            result: result(ToolResultStatus::Succeeded, 71),
        },
    );
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCompleted {
            ordinal: ToolOrdinal::new(0),
            result: result(ToolResultStatus::Succeeded, 70),
        },
    );
    let slots = state.tools().expect("batch").slots();
    assert_eq!(slots[0].proposal().ordinal().get(), 0);
    assert_eq!(slots[1].proposal().ordinal().get(), 1);
    assert_eq!(slots[0].result().expect("result").result_digest(), digest(70));
    assert_eq!(slots[1].result().expect("result").result_digest(), digest(71));
    apply(&mut state, &mut events, AgentCommandKind::ResultRecordingStarted);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ResultsRecorded { transcript_digest: digest(27) },
    );
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::PreparingContext));
    assert_eq!(replay(&events).expect("replay"), state);
}

#[test]
fn unordered_and_parallel_mutation_batches_are_rejected_without_state_change() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    let before = state.clone();
    let command = AgentCommand::new(
        id16(80, peritus_types::CommandId::new),
        id16(81, peritus_types::EventId::new),
        state.logical_revision(),
        state.state_digest(),
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(1, ToolSideEffect::None), tool(0, ToolSideEffect::None)],
        },
    );
    assert_eq!(
        reduce(&state, &command).expect_err("order").code(),
        AgentErrorCode::NonCanonicalOrder
    );
    assert_eq!(state, before);

    let command = AgentCommand::new(
        id16(82, peritus_types::CommandId::new),
        id16(83, peritus_types::EventId::new),
        state.logical_revision(),
        state.state_digest(),
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(0, ToolSideEffect::Workspace), tool(1, ToolSideEffect::None)],
        },
    );
    assert_eq!(
        reduce(&state, &command).expect_err("serialization").code(),
        AgentErrorCode::InvalidTool
    );
    assert_eq!(state, before);
}

#[test]
fn indeterminate_result_is_renderable_but_cannot_claim_gate_ready_completion() {
    let (mut events, mut state) = started(64);
    start_model(&mut state, &mut events);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(0, ToolSideEffect::Process)],
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::AuthorizationStarted);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ToolAuthorized {
            ordinal: ToolOrdinal::new(0),
            authority_digest: digest(84),
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::ToolExecutionStarted);
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
            result: result(ToolResultStatus::Indeterminate, 85),
        },
    );
    apply(&mut state, &mut events, AgentCommandKind::ResultRecordingStarted);
    apply(
        &mut state,
        &mut events,
        AgentCommandKind::ResultsRecorded { transcript_digest: digest(27) },
    );
    assert!(state.has_unresolved_indeterminate());
    assert_eq!(state.phase(), AgentPhase::Active(ActivePhase::PreparingContext));
    assert_eq!(replay(&events).expect("replay"), state);
}
