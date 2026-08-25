//! Deterministic classification of one pending directive after restart.

use crate::{DirectiveDeliveryState, DirectiveId, OrchestratorError, PendingDirective};

use super::{ChildProjectionPort, ChildReconciliation};

/// Loads canonical live heads for a Pause command at the current E0 checkpoint.
///
/// # Errors
///
/// Returns an error when a child projection cannot be loaded or is not the expected active head.
pub fn collect_pause_reconciliation(
    state: &crate::OrchestratorState,
    children: &mut impl ChildProjectionPort,
) -> Result<crate::ResumeReconciliation, OrchestratorError> {
    let mut heads = Vec::with_capacity(state.active_children().len());
    for kind in state.active_children() {
        let head = children.active_head(*kind)?;
        if head.aggregate() != *kind || head.terminal().is_some() {
            return Err(OrchestratorError::new(
                crate::OrchestratorErrorKind::BindingMismatch,
                crate::OrchestratorRecoveryAction::ReconcileChild,
                "active child projection returned another aggregate or terminal head",
            ));
        }
        heads.push(head);
    }
    crate::ResumeReconciliation::from_checkpoint(state, heads)
}

/// Reloads live heads and proves they still equal the reconciliation committed by Pause.
///
/// # Errors
///
/// Returns an error when the pause checkpoint is absent or any live child head changed.
pub fn verify_resume_reconciliation(
    state: &crate::OrchestratorState,
    children: &mut impl ChildProjectionPort,
) -> Result<crate::ResumeReconciliation, OrchestratorError> {
    let expected = state.paused_reconciliation().ok_or_else(|| {
        OrchestratorError::new(
            crate::OrchestratorErrorKind::MissingCheckpoint,
            crate::OrchestratorRecoveryAction::ReconcileChild,
            "paused E0 state lacks its child-head reconciliation",
        )
    })?;
    let mut heads = Vec::with_capacity(state.active_children().len());
    for kind in state.active_children() {
        heads.push(children.active_head(*kind)?);
    }
    let observed =
        crate::ResumeReconciliation::from_wire(expected.checkpoint_state_digest(), heads)?;
    if &observed == expected {
        Ok(observed)
    } else {
        Err(OrchestratorError::new(
            crate::OrchestratorErrorKind::StaleState,
            crate::OrchestratorRecoveryAction::ReconcileChild,
            "live child heads changed after the committed pause checkpoint",
        ))
    }
}

/// Closed recovery action for one committed pending directive.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PendingDirectiveClass {
    /// The same stable directive may be delivered or redelivered.
    Deliverable,
    /// The child owns exact active work and E0 must await its terminal.
    AcknowledgedAwaitingResult,
    /// Exact terminal truth exists but has not yet been recorded by E0.
    CompletedAwaitingObservation,
    /// Child identity or revision conflicts with the committed directive.
    StaleConflicting,
    /// Ownership or terminal truth is irreconcilably ambiguous.
    TerminalAmbiguous,
}

/// One authority-free restart classification and optional checked child observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    directive_id: DirectiveId,
    class: PendingDirectiveClass,
    observation: Option<crate::ChildObservation>,
}

impl RecoveryReport {
    /// Returns the committed directive being classified.
    #[must_use]
    pub const fn directive_id(&self) -> DirectiveId {
        self.directive_id
    }
    /// Returns the closed recovery classification.
    #[must_use]
    pub const fn class(&self) -> PendingDirectiveClass {
        self.class
    }
    /// Returns checked terminal or cancellation truth ready for reducer admission.
    #[must_use]
    pub const fn observation(&self) -> Option<&crate::ChildObservation> {
        self.observation.as_ref()
    }
}

/// Classifies one pending directive without delivering it or advancing E0 state.
///
/// # Errors
/// Returns a typed child-projection load failure. Ambiguous observed truth is a successful report.
pub fn classify_pending_directive(
    directive: &PendingDirective,
    children: &mut impl ChildProjectionPort,
) -> Result<RecoveryReport, OrchestratorError> {
    if directive.delivery_state() == DirectiveDeliveryState::Ready {
        return Ok(report(directive.id(), PendingDirectiveClass::Deliverable, None));
    }
    let reconciled = children.reconcile(directive)?;
    let (class, observation) = match (directive.delivery_state(), reconciled) {
        (DirectiveDeliveryState::Published, ChildReconciliation::Absent) => {
            (PendingDirectiveClass::Deliverable, None)
        }
        (
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged,
            ChildReconciliation::Active,
        ) => (PendingDirectiveClass::AcknowledgedAwaitingResult, None),
        (
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged,
            ChildReconciliation::Completed(observation),
        ) => (PendingDirectiveClass::CompletedAwaitingObservation, Some(*observation)),
        (
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged,
            ChildReconciliation::Classified(classification),
        ) => (
            PendingDirectiveClass::CompletedAwaitingObservation,
            Some(crate::ChildObservation::CancellationClassification(classification)),
        ),
        (
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged,
            ChildReconciliation::Conflicting,
        ) => (PendingDirectiveClass::StaleConflicting, None),
        (
            DirectiveDeliveryState::Published | DirectiveDeliveryState::Acknowledged,
            ChildReconciliation::Ambiguous,
        )
        | (DirectiveDeliveryState::Acknowledged, ChildReconciliation::Absent) => {
            (PendingDirectiveClass::TerminalAmbiguous, None)
        }
        (DirectiveDeliveryState::Ready, _) => (PendingDirectiveClass::Deliverable, None),
    };
    Ok(report(directive.id(), class, observation))
}

const fn report(
    directive_id: DirectiveId,
    class: PendingDirectiveClass,
    observation: Option<crate::ChildObservation>,
) -> RecoveryReport {
    RecoveryReport { directive_id, class, observation }
}
