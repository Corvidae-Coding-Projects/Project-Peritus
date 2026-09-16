//! Actual command/event phase transitions and their weaker tag projection.

mod common;

use common::*;
use peritus_agent::{
    ActivePhase, AgentCommand, AgentCommandKind, AgentErrorCode, AgentFailure, AgentFailureKind,
    AgentLimitDimension, AgentPhase, AgentRecovery, ProviderEventRecord, ProviderRetryClass,
    ProviderRetryRecord, SafeText, TerminalKind, ToolResultStatus, ToolSideEffect,
    phase_transition_valid, reduce, replay,
};
use peritus_types::{CommandId, EventId};

#[test]
fn phase_tag_projection_contains_exactly_the_possible_phase_pairs() {
    let mut expected = Vec::from([
        (0, 1),
        (1, 2),
        (2, 1),
        (2, 2),
        (2, 3),
        (2, 7),
        (3, 4),
        (4, 4),
        (4, 5),
        (5, 5),
        (5, 6),
        (6, 0),
        (9, 12),
        (7, 10),
    ]);
    expected.extend((0..=7).map(|before| (before, 8)));
    expected.extend((0..=7).map(|after| (8, after)));
    expected.extend((0..=8).map(|before| (before, 9)));
    expected.extend((0..=9).map(|before| (before, 11)));

    for before in 0..=13 {
        for after in 0..=13 {
            assert_eq!(
                phase_transition_valid(before, after),
                expected.contains(&(before, after)),
                "phase pair {before}->{after}",
            );
        }
    }
}

#[test]
fn actual_normal_command_path_uses_each_expected_phase_edge_and_replays() {
    let (mut events, mut state) = started(64);
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ContextPrepared(context()),
        AgentPhase::Active(ActivePhase::RequestingModel),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ModelRequestStarted {
            call_id: peritus_agent::ModelCallId::new(digest(24)).expect("model call"),
            request_digest: digest(25),
        },
        AgentPhase::Active(ActivePhase::StreamingResponse),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ProviderEventObserved(ProviderEventRecord::new(1, digest(28), 0, false)),
        AgentPhase::Active(ActivePhase::StreamingResponse),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCallsProposed {
            terminal: terminal(),
            proposals: vec![tool(0, ToolSideEffect::Workspace)],
        },
        AgentPhase::Active(ActivePhase::ProposedToolCalls),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::AuthorizationStarted,
        AgentPhase::Active(ActivePhase::AwaitingAuthorization),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolAuthorized {
            ordinal: peritus_agent::ToolOrdinal::new(0),
            authority_digest: digest(60),
        },
        AgentPhase::Active(ActivePhase::AwaitingAuthorization),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolExecutionStarted,
        AgentPhase::Active(ActivePhase::ExecutingTools),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolDispatched { ordinal: peritus_agent::ToolOrdinal::new(0) },
        AgentPhase::Active(ActivePhase::ExecutingTools),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolActivated { ordinal: peritus_agent::ToolOrdinal::new(0) },
        AgentPhase::Active(ActivePhase::ExecutingTools),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ToolCompleted {
            ordinal: peritus_agent::ToolOrdinal::new(0),
            result: result(ToolResultStatus::Succeeded, 61),
        },
        AgentPhase::Active(ActivePhase::ExecutingTools),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ResultRecordingStarted,
        AgentPhase::Active(ActivePhase::RecordingResults),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::ResultsRecorded { transcript_digest: digest(27) },
        AgentPhase::Active(ActivePhase::PreparingContext),
    );
    start_model(&mut state, &mut events);
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::CompletionProposed { terminal: terminal(), proposal: proposal() },
        AgentPhase::Active(ActivePhase::ProposedCompletion),
    );
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::CompletionCommitted,
        AgentPhase::Terminal(TerminalKind::Completed),
    );

    assert_eq!(replay(&events).expect("normal phase-path replay"), state);
}

#[test]
fn failure_and_exhaustion_terminate_supported_nonterminal_paths_and_replay() {
    for prefix in [FailurePrefix::Active, FailurePrefix::Paused, FailurePrefix::Cancelling] {
        let (mut events, mut state) = started(16);
        match prefix {
            FailurePrefix::Active => {}
            FailurePrefix::Paused => apply(&mut state, &mut events, AgentCommandKind::Paused),
            FailurePrefix::Cancelling => {
                apply(&mut state, &mut events, AgentCommandKind::CancellationRequested);
            }
        }
        apply_phase(
            &mut state,
            &mut events,
            AgentCommandKind::Failed(AgentFailure::new(
                AgentFailureKind::Provider,
                SafeText::new("bounded provider failure".to_owned()).expect("failure"),
            )),
            AgentPhase::Terminal(TerminalKind::Failed),
        );
        assert_eq!(replay(&events).expect("failed path replay"), state);
    }

    let (mut events, mut state) = started(16);
    apply_phase(
        &mut state,
        &mut events,
        AgentCommandKind::Exhausted(AgentFailure::new(
            AgentFailureKind::Exhausted(AgentLimitDimension::ProviderEvents),
            SafeText::new("bounded provider-event exhaustion".to_owned()).expect("exhaustion"),
        )),
        AgentPhase::Terminal(TerminalKind::Failed),
    );
    assert_eq!(replay(&events).expect("exhausted path replay"), state);
}

#[test]
fn command_phase_rejections_keep_error_precedence_and_original_state() {
    let (_, state) = started(16);
    assert_rejected_without_mutation(
        &state,
        AgentCommandKind::ProviderRetryScheduled(ProviderRetryRecord::new(
            digest(70),
            digest(71),
            ProviderRetryClass::SafeNewRequest,
        )),
        AgentErrorCode::IllegalPhase,
        AgentRecovery::CorrectRequest,
        "command is illegal in the current phase",
    );
    assert_rejected_without_mutation(
        &state,
        AgentCommandKind::Failed(AgentFailure::new(
            AgentFailureKind::Exhausted(AgentLimitDimension::ProviderEvents),
            SafeText::new("mismatched failure command".to_owned()).expect("failure"),
        )),
        AgentErrorCode::InvalidCommand,
        AgentRecovery::CorrectRequest,
        "failure command does not match failure kind",
    );
}

#[derive(Clone, Copy)]
enum FailurePrefix {
    Active,
    Paused,
    Cancelling,
}

fn apply_phase(
    state: &mut peritus_agent::AgentTurnState,
    events: &mut Vec<peritus_agent::AgentEvent>,
    kind: AgentCommandKind,
    expected: AgentPhase,
) {
    let before = state.phase();
    apply(state, events, kind);
    assert_eq!(state.phase(), expected, "successor from {before:?}");
    assert!(phase_transition_valid(before.tag(), expected.tag()));
    let event = events.last().expect("phase event");
    assert_eq!(event.prior_phase(), Some(before));
}

fn assert_rejected_without_mutation(
    state: &peritus_agent::AgentTurnState,
    kind: AgentCommandKind,
    expected_code: AgentErrorCode,
    expected_recovery: AgentRecovery,
    expected_detail: &str,
) {
    let before = state.clone();
    let command = AgentCommand::new(
        id16(220, CommandId::new),
        id16(221, EventId::new),
        state.logical_revision(),
        state.state_digest(),
        kind,
    );
    let rejection = reduce(state, &command).expect_err("phase command rejected");
    assert_eq!(rejection.code(), expected_code);
    assert_eq!(rejection.recovery(), expected_recovery);
    assert_eq!(rejection.detail(), expected_detail);
    assert_eq!(rejection.phase(), Some(state.phase()));
    assert_eq!(state, &before);
    assert_eq!(state.canonical_bytes(), before.canonical_bytes());
}
