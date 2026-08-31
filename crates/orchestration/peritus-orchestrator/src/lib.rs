//! Durable deterministic E0 delivery orchestration for Peritus.

mod acceptance;
mod binding;
mod candidate;
pub(crate) mod canonical;
mod child;
mod command;
mod directive;
mod durability;
mod error;
mod event;
mod handoff;
mod identity;
#[cfg(test)]
mod integration_tests;
mod limits;
mod ownership;
mod phase;
#[cfg(not(verus_only))]
mod product;
mod projection;
#[cfg(not(verus_only))]
pub mod qualification;
pub(crate) mod reducer;
mod replay;
pub mod runtime;
pub(crate) mod state;
mod terminal;
mod verified;
pub(crate) mod wire;

pub use acceptance::{AcceptanceCertificate, KernelAcceptancePlan};
pub use binding::{OrchestratorBinding, QualityCycleBinding};
pub use candidate::CandidateBinding;
pub use child::{
    AgentChildObservation, CancellationChildClassification, CancellationClassificationKind,
    ChildAggregateKind, ChildHead, ChildObservation, ChildTerminalClass,
    CollaborationChildObservation, FixerResponseIdentity, GateChildObservation,
    GateObservationClass, HandoffActivationObservation, KernelAcceptanceObservation,
    KernelAcceptanceOutcome, ReviewChildObservation, ReviewFixerObservation, ReviewFixerRecord,
    ReviewObservationClass, SchedulerChildObservation,
};
pub use command::{
    FixerCompletion, OrchestratorCommand, OrchestratorCommandKind, OrchestratorGenesis,
    ResumeReconciliation,
};
pub use directive::{
    DirectiveDeliveryState, DirectiveDestination, DirectiveId, DirectiveKind,
    DirectivePayloadBinding, PendingDirective, directive_payload_digest,
};
pub use durability::{
    ClaimedDirectiveAcknowledgement, ORCHESTRATOR_STATE_NAMESPACE,
    commit_claimed_directive_acknowledgement, commit_orchestrator_transition,
    load_orchestrator_replay, orchestrator_aggregate_key, orchestrator_state_key,
};
pub use error::{OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction};
pub use event::{OrchestratorEvent, OrchestratorEventKind, OrchestratorTransition};
pub use handoff::{Handoff, HandoffKind, HandoffRole};
pub use identity::{HandoffId, OrchestratorId};
pub use limits::OrchestratorLimits;
pub use ownership::{RoleAssignment, RoleOwnership};
pub use phase::{ActivePhase, OrchestratorPhase};
#[cfg(not(verus_only))]
pub use product::{ProductionDecision, ProductionRunCoordinator};
pub use projection::OrchestratorProjection;
pub use reducer::{decide, replay, start};
pub use replay::OrchestratorReplay;
pub use state::{OrchestratorCounters, OrchestratorState};
pub use terminal::{OrchestratorTerminal, OrchestratorTerminalKind, TerminalCause};
pub use verified::{
    cancellation_dominates, candidate_cycles_are_fresh, counters_are_bounded, replay_equivalent,
    roles_are_separated, terminal_is_truthful, transition_is_legal,
};
pub use wire::{OrchestratorCommandFrame, OrchestratorEventFrame, OrchestratorStateFrame};
