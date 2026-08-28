//! Durable deterministic acceptance gate orchestration for Peritus.
//!
//! D1 consumes checked B2 acceptance declarations, clean C1 snapshot observations, authorized C4
//! quality terminals, and C0 durability. It never owns raw process, shell, workspace-mutation, or
//! acceptance-override authority.

mod canonical;
mod command;
mod descriptor;
mod durability;
mod engine;
mod error;
mod event;
mod evidence;
mod outcome;
#[cfg(not(verus_only))]
mod product;
mod projection;
mod reducer;
mod state;
mod verified;
mod wire;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use command::{GateCommand, GateCommandKind, RecoveryDisposition};
pub use descriptor::{GatePlan, PlannedGate};
pub use durability::{
    GATE_STATE_NAMESPACE, GateReplay, commit_gate_lifecycle_transition, commit_gate_transition,
    gate_aggregate_key, gate_state_key, load_gate_replay,
};
pub use engine::{
    DispatchReceipt, GateDispatch, GateEngine, GateExecutor, GateRecovery, RecoveryReceipt,
    observed_result_kind, recovery_kind,
};
pub use error::{GateError, GateErrorKind, GateRecoveryAction, GateRejection};
pub use event::{GateEvent, GateEventKind, GateTransition};
pub use evidence::{
    EvidencePublication, GateEvidencePublisher, GateEvidenceReceipt, PublishedGateEvidence,
};
pub use outcome::{
    GateArtifact, GateAttemptResult, GateOutcomeKind, RecoveryRequirement, RetryPermission,
};
#[cfg(not(verus_only))]
pub use product::{
    AffectedProject, GateCommandSpec, GateExecutionRecord, ProjectKind, TargetGatePlan,
    TargetGateReport,
};
pub use projection::{GateProjection, ProjectedGate, ProjectedRun};
pub use reducer::{decide, replay, start};
pub use state::{
    ActiveAttempt, GateResumePhase, GateRunPhase, GateRunState, GateSlot, GateSlotPhase,
    GateTerminal, GateTerminalKind,
};
pub use verified::{
    attempts_are_bounded, dependencies_are_satisfied, dependency_order_is_legal, evidence_is_fresh,
    no_implicit_success, replay_equivalent, terminal_truthful,
};
pub use wire::GateCommandFrame;
