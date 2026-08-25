//! Checked local hard-limit and explicit exhaustion tests.

mod common;

use common::*;
use peritus_agent::{
    AgentCommand, AgentCommandKind, AgentErrorCode, AgentFailure, AgentFailureKind,
    AgentLimitDimension, AgentLimits, AgentRecovery, ModelCallId, SafeText, TerminalKind, reduce,
    replay,
};

#[test]
fn transition_exhaustion_is_explicit_failure_not_completion() {
    let (mut events, mut state) = started(2);
    apply(&mut state, &mut events, AgentCommandKind::ContextPrepared(context()));
    let command = AgentCommand::new(
        id16(70, peritus_types::CommandId::new),
        id16(71, peritus_types::EventId::new),
        state.logical_revision(),
        state.state_digest(),
        AgentCommandKind::ModelRequestStarted {
            call_id: ModelCallId::new(digest(72)).expect("call"),
            request_digest: digest(73),
        },
    );
    let rejection = reduce(&state, &command).expect_err("transition budget");
    assert_eq!(rejection.code(), AgentErrorCode::LimitExceeded);
    assert_eq!(rejection.recovery(), AgentRecovery::Exhausted);

    let failure = AgentFailure::new(
        AgentFailureKind::Exhausted(AgentLimitDimension::Transitions),
        SafeText::new("transition budget exhausted".to_owned()).expect("detail"),
    );
    apply(&mut state, &mut events, AgentCommandKind::Exhausted(failure));
    assert_eq!(state.terminal_kind(), Some(TerminalKind::Failed));
    assert!(state.completion().is_none());
    assert_eq!(state.counters().transitions(), 2);
    assert_eq!(replay(&events).expect("replay"), state);
}

#[test]
fn checked_limits_reject_zero_and_hard_ceiling_overflow() {
    assert_eq!(
        AgentLimits::new(0, 1, 1, 1, 1, 1, 1).expect_err("zero").code(),
        AgentErrorCode::InvalidLimit,
    );
    assert_eq!(
        AgentLimits::new(1, 1, 1, AgentLimits::HARD_MAX_BYTES + 1, 1, 1, 1)
            .expect_err("ceiling")
            .code(),
        AgentErrorCode::InvalidLimit,
    );
}
