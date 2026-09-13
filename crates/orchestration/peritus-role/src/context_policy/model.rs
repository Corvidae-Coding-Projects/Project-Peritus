//! Input-defined complete role policy tables.
use super::{ContextClass, PresentationStyle};
use peritus_policy::ActorRole;
use vstd::prelude::*;

verus! {
/// Exact canonical visible class sequence for a supplied role.
pub open spec fn visible(role: ActorRole) -> Seq<ContextClass> {
    match role {
        ActorRole::Writer | ActorRole::Fixer | ActorRole::EvolutionAgent => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositoryInstructions, ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::MemoryEvidence, ContextClass::PriorFinding, ContextClass::FindingResolution, ContextClass::AgentProgress, ContextClass::HiddenReasoning]@,
        ActorRole::Reviewer => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositoryInstructions, ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::PriorFinding, ContextClass::FindingResolution, ContextClass::AgentProgress]@,
        ActorRole::Evaluator => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::MemoryEvidence, ContextClass::PriorFinding, ContextClass::FindingResolution, ContextClass::AgentProgress]@,
        ActorRole::GateRunner => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::AgentProgress]@,
        _ => [ContextClass::ImmutablePolicy, ContextClass::WorkspaceState, ContextClass::AgentProgress]@,
    }
}

/// Exact canonical required class sequence for a supplied role.
pub open spec fn required(role: ActorRole) -> Seq<ContextClass> {
    match role {
        ActorRole::Writer | ActorRole::EvolutionAgent => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositorySource, ContextClass::WorkspaceState]@,
        ActorRole::Fixer => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositorySource, ContextClass::WorkspaceState, ContextClass::PriorFinding]@,
        ActorRole::Reviewer => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::GateEvidence]@,
        ActorRole::Evaluator => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::ActiveUserRequest, ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence]@,
        ActorRole::GateRunner => [ContextClass::ImmutablePolicy, ContextClass::AcceptanceSpecification, ContextClass::WorkspaceState, ContextClass::GateEvidence]@,
        _ => [ContextClass::ImmutablePolicy, ContextClass::WorkspaceState]@,
    }
}

/// Exact canonical contributable class sequence for a supplied role.
pub open spec fn contributable(role: ActorRole) -> Seq<ContextClass> {
    match role {
        ActorRole::Writer | ActorRole::Fixer => [ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::AgentProgress, ContextClass::HiddenReasoning]@,
        ActorRole::Reviewer => [ContextClass::ToolObservation, ContextClass::PriorFinding, ContextClass::AgentProgress]@,
        ActorRole::Evaluator => [ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::AgentProgress]@,
        ActorRole::EvolutionAgent => [ContextClass::RepositorySource, ContextClass::CandidateDiff, ContextClass::WorkspaceState, ContextClass::GateEvidence, ContextClass::ToolObservation, ContextClass::MemoryEvidence, ContextClass::PriorFinding, ContextClass::FindingResolution, ContextClass::AgentProgress, ContextClass::HiddenReasoning]@,
        _ => Seq::empty().push(ContextClass::AgentProgress),
    }
}

/// Exact presentation style selected by role.
pub open spec fn style(role: ActorRole) -> PresentationStyle {
    match role {
        ActorRole::Writer => PresentationStyle::Implementation,
        ActorRole::Reviewer => PresentationStyle::AdversarialReview,
        ActorRole::Fixer => PresentationStyle::FindingResolution,
        ActorRole::Evaluator => PresentationStyle::IsolatedEvaluation,
        ActorRole::EvolutionAgent => PresentationStyle::HarnessEvolution,
        _ => PresentationStyle::Restricted,
    }
}
} // verus!
