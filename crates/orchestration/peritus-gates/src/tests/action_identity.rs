use crate::test_support as support;
use crate::{GateCommandKind, GateErrorKind, GateRejection, GateSlotPhase, RecoveryDisposition};

#[test]
fn retry_rejects_reused_action_with_fresh_execution_without_mutation() {
    let fixture = support::fixture(2);
    let started =
        crate::start(&fixture.plan, &support::start_command(&fixture, 130)).expect("start");
    let mut events = vec![started.event().clone()];
    let mut state = started.into_state();
    let first = support::attempt(&fixture, 131, 1);
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        131,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: first },
    );
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        132,
        GateCommandKind::MarkDispatched {
            gate_id: fixture.first,
            execution_id: first.execution_id(),
        },
    );
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        133,
        GateCommandKind::ObserveResult {
            gate_id: fixture.first,
            execution_id: first.execution_id(),
            result: support::retryable(fixture.first, 133),
        },
    );
    super::advance_kind(
        &fixture.plan,
        &mut state,
        &mut events,
        134,
        GateCommandKind::ClassifyRecovery {
            gate_id: fixture.first,
            execution_id: first.execution_id(),
            disposition: RecoveryDisposition::SafeToRetry,
        },
    );
    assert_eq!(state.slot(fixture.first).expect("slot").phase(), GateSlotPhase::RetryPending);

    let reused_action = support::attempt_with_action(&fixture, 135, 2, first.action_id());
    assert_ne!(reused_action.execution_id(), first.execution_id());
    let before = state.clone();
    let command = support::command(
        &state,
        135,
        GateCommandKind::PrepareAttempt { gate_id: fixture.first, attempt: reused_action },
    );
    let error = crate::decide(&fixture.plan, &state, &command).expect_err("reused action");
    assert_eq!(error.kind(), GateErrorKind::Rejected(GateRejection::IllegalRetry));
    assert_eq!(state, before);
    assert_eq!(state.used_executions(), [first.execution_id()]);
    assert_eq!(state.used_actions(), [first.action_id()]);

    let replayed = crate::replay(&fixture.plan, &events).expect("replay");
    assert_eq!(replayed, state);
    assert_eq!(replayed.used_actions(), [first.action_id()]);
}
