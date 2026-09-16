//! Public reducer error precedence and unchanged-state coverage for control commands.

mod common;

use common::*;
use peritus_agent::{
    AgentCommand, AgentCommandKind, AgentErrorCode, AgentOperation, AgentPhase, AgentRecovery,
    AgentTurnState, reduce,
};
use peritus_types::{CommandId, EventId};

#[test]
fn resume_and_pause_rejections_keep_existing_classification_and_state() {
    let (mut events, mut state) = started(64);
    assert_rejected(
        &state,
        AgentCommandKind::Resumed { recovery_checked: false },
        AgentRecovery::CorrectRequest,
        "command is illegal in the current phase",
    );
    assert_rejected(
        &state,
        AgentCommandKind::CancellationFinished,
        AgentRecovery::CorrectRequest,
        "command is illegal in the current phase",
    );
    apply(&mut state, &mut events, AgentCommandKind::Paused);
    assert_rejected(
        &state,
        AgentCommandKind::Resumed { recovery_checked: false },
        AgentRecovery::RestartTurn,
        "resume requires completed recovery checks",
    );
    assert_rejected(
        &state,
        AgentCommandKind::Paused,
        AgentRecovery::CorrectRequest,
        "command is illegal in the current phase",
    );
}

#[test]
fn cancelling_rejects_reentry_and_terminal_outer_guard_keeps_precedence() {
    let (mut events, mut state) = started(64);
    apply(&mut state, &mut events, AgentCommandKind::CancellationRequested);
    for kind in [
        AgentCommandKind::Paused,
        AgentCommandKind::Resumed { recovery_checked: false },
        AgentCommandKind::CancellationRequested,
    ] {
        assert_rejected(
            &state,
            kind,
            AgentRecovery::CorrectRequest,
            "command is illegal in the current phase",
        );
    }
    apply(&mut state, &mut events, AgentCommandKind::CancellationFinished);
    for kind in [
        AgentCommandKind::Paused,
        AgentCommandKind::Resumed { recovery_checked: false },
        AgentCommandKind::CancellationRequested,
        AgentCommandKind::CancellationFinished,
    ] {
        assert_rejected(
            &state,
            kind,
            AgentRecovery::Terminal,
            "terminal state rejects all commands",
        );
    }
}

fn assert_rejected(
    state: &AgentTurnState,
    kind: AgentCommandKind,
    recovery: AgentRecovery,
    detail: &str,
) {
    let before = state.clone();
    let command = AgentCommand::new(
        id16(200, CommandId::new),
        id16(201, EventId::new),
        state.logical_revision(),
        state.state_digest(),
        kind,
    );
    let error = reduce(state, &command).expect_err("rejected control command");
    let code = if recovery == AgentRecovery::RestartTurn {
        AgentErrorCode::InvalidCommand
    } else {
        AgentErrorCode::IllegalPhase
    };
    assert_eq!(error.code(), code);
    assert_eq!(error.operation(), AgentOperation::Reduce);
    assert_eq!(error.recovery(), recovery);
    assert_eq!(error.detail(), detail);
    assert_eq!(error.turn_id(), Some(state.binding().turn_id()));
    assert_eq!(error.phase(), Some(state.phase()));
    assert_eq!(state, &before);
    assert_eq!(state.canonical_bytes(), before.canonical_bytes());
}

#[test]
fn phase_tags_and_terminal_detection_remain_const_and_stable() {
    const PAUSED_TAG: u8 = AgentPhase::Paused.tag();
    const PAUSED_TERMINAL: bool = AgentPhase::Paused.is_terminal();
    assert_eq!(PAUSED_TAG, 8);
    let terminal = PAUSED_TERMINAL;
    assert!(!terminal);
    for (kind, expected) in [
        (peritus_agent::TerminalKind::Completed, 10),
        (peritus_agent::TerminalKind::Failed, 11),
        (peritus_agent::TerminalKind::Cancelled, 12),
    ] {
        assert_eq!(AgentPhase::Terminal(kind).tag(), expected);
        assert!(AgentPhase::Terminal(kind).is_terminal());
    }
}
