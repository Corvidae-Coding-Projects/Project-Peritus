//! Checked E0 identity and independently bounded limit behavior.

use peritus_orchestrator::{
    HandoffId, OrchestratorErrorKind, OrchestratorId, OrchestratorLimits,
    OrchestratorRecoveryAction,
};

fn production_limits() -> OrchestratorLimits {
    OrchestratorLimits::new(8, 4, 12, 16, 20, 32, 48, 64, 96, 128, 1_048_576, 2_097_152)
        .expect("fixture limits are independently bounded")
}

#[test]
fn checked_identities_reject_zero_and_preserve_canonical_bytes() {
    let orchestrator = OrchestratorId::new([7; OrchestratorId::LENGTH])
        .expect("nonzero orchestrator identity is valid");
    let handoff =
        HandoffId::new([9; HandoffId::LENGTH]).expect("nonzero handoff identity is valid");

    assert_eq!(orchestrator.as_bytes(), &[7; 16]);
    assert_eq!(orchestrator.into_bytes(), [7; 16]);
    assert_eq!(handoff.as_bytes(), &[9; 16]);
    assert_eq!(handoff.into_bytes(), [9; 16]);

    for error in [
        OrchestratorId::new([0; 16]).expect_err("zero orchestrator identity is reserved"),
        HandoffId::new([0; 16]).expect_err("zero handoff identity is reserved"),
    ] {
        assert_eq!(error.kind(), OrchestratorErrorKind::InvalidInput);
        assert_eq!(error.recovery(), OrchestratorRecoveryAction::CorrectInput);
    }
}

#[test]
fn limits_retain_each_independent_dimension() {
    let limits = production_limits();

    assert_eq!(limits.revisions(), 8);
    assert_eq!(limits.writer_cycles(), 4);
    assert_eq!(limits.fixer_cycles(), 12);
    assert_eq!(limits.gate_cycles(), 16);
    assert_eq!(limits.review_cycles(), 20);
    assert_eq!(limits.handoffs(), 32);
    assert_eq!(limits.child_directives(), 48);
    assert_eq!(limits.retained_observations(), 64);
    assert_eq!(limits.artifact_references(), 96);
    assert_eq!(limits.cancellation_reconciliations(), 128);
    assert_eq!(limits.event_bytes(), 1_048_576);
    assert_eq!(limits.state_bytes(), 2_097_152);
}

#[test]
fn every_limit_rejects_zero_or_compiled_ceiling_excess() {
    let zero_revision = OrchestratorLimits::new(0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1)
        .expect_err("zero revision limit is invalid");
    let oversized_event = OrchestratorLimits::new(
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        1,
        OrchestratorLimits::MAX_EVENT_BYTES + 1,
        1,
    )
    .expect_err("event limit above the compiled ceiling is invalid");

    for error in [zero_revision, oversized_event] {
        assert_eq!(error.kind(), OrchestratorErrorKind::LimitExceeded);
        assert_eq!(error.recovery(), OrchestratorRecoveryAction::CorrectInput);
    }
}
