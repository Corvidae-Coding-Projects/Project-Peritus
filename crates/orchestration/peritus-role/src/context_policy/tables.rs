//! Production context-policy constructors and canonical tables.
use super::{
    ContextClass, ContextClassSet, ContextPolicy, MemoryVisibility, PresentationProfile,
    PresentationStyle, ReasoningVisibility,
};
use peritus_policy::ActorRole;

pub(super) fn policy_for(role: ActorRole) -> ContextPolicy {
    match role {
        ActorRole::Writer => writer_policy(),
        ActorRole::Reviewer => reviewer_policy(),
        ActorRole::Fixer => fixer_policy(),
        ActorRole::Evaluator => evaluator_policy(),
        ActorRole::EvolutionAgent => evolver_policy(),
        _ => restricted_policy(role),
    }
}
fn writer_policy() -> ContextPolicy {
    ContextPolicy {
        visible: ContextClassSet::from_canonical(all_classes()),
        contributable: ContextClassSet::from_canonical(vec![
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::AgentProgress,
            ContextClass::HiddenReasoning,
        ]),
        required: base_required(),
        fresh_context: false,
        memory_visibility: MemoryVisibility::EvidenceBacked,
        reasoning_visibility: ReasoningVisibility::SameLineageOnly,
        allow_producer_ancestry: true,
        presentation: PresentationProfile::new(PresentationStyle::Implementation),
    }
}
fn reviewer_policy() -> ContextPolicy {
    ContextPolicy {
        visible: ContextClassSet::from_canonical(vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::ActiveUserRequest,
            ContextClass::RepositoryInstructions,
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::PriorFinding,
            ContextClass::FindingResolution,
            ContextClass::AgentProgress,
        ]),
        contributable: ContextClassSet::from_canonical(vec![
            ContextClass::ToolObservation,
            ContextClass::PriorFinding,
            ContextClass::AgentProgress,
        ]),
        required: ContextClassSet::from_canonical(vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::ActiveUserRequest,
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::GateEvidence,
        ]),
        fresh_context: true,
        memory_visibility: MemoryVisibility::Excluded,
        reasoning_visibility: ReasoningVisibility::Excluded,
        allow_producer_ancestry: false,
        presentation: PresentationProfile::new(PresentationStyle::AdversarialReview),
    }
}
fn fixer_policy() -> ContextPolicy {
    let mut policy = writer_policy();
    policy.required = ContextClassSet::from_canonical(vec![
        ContextClass::ImmutablePolicy,
        ContextClass::AcceptanceSpecification,
        ContextClass::ActiveUserRequest,
        ContextClass::RepositorySource,
        ContextClass::WorkspaceState,
        ContextClass::PriorFinding,
    ]);
    policy.presentation = PresentationProfile::new(PresentationStyle::FindingResolution);
    policy
}
fn evaluator_policy() -> ContextPolicy {
    ContextPolicy {
        visible: ContextClassSet::from_canonical(vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::ActiveUserRequest,
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::MemoryEvidence,
            ContextClass::PriorFinding,
            ContextClass::FindingResolution,
            ContextClass::AgentProgress,
        ]),
        contributable: ContextClassSet::from_canonical(vec![
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::AgentProgress,
        ]),
        required: ContextClassSet::from_canonical(vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::ActiveUserRequest,
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
        ]),
        fresh_context: true,
        memory_visibility: MemoryVisibility::EvidenceBacked,
        reasoning_visibility: ReasoningVisibility::Excluded,
        allow_producer_ancestry: false,
        presentation: PresentationProfile::new(PresentationStyle::IsolatedEvaluation),
    }
}
fn evolver_policy() -> ContextPolicy {
    ContextPolicy {
        visible: ContextClassSet::from_canonical(all_classes()),
        contributable: ContextClassSet::from_canonical(vec![
            ContextClass::RepositorySource,
            ContextClass::CandidateDiff,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::MemoryEvidence,
            ContextClass::PriorFinding,
            ContextClass::FindingResolution,
            ContextClass::AgentProgress,
            ContextClass::HiddenReasoning,
        ]),
        required: base_required(),
        fresh_context: true,
        memory_visibility: MemoryVisibility::EvidenceBacked,
        reasoning_visibility: ReasoningVisibility::SameLineageOnly,
        allow_producer_ancestry: true,
        presentation: PresentationProfile::new(PresentationStyle::HarnessEvolution),
    }
}
fn restricted_policy(role: ActorRole) -> ContextPolicy {
    let visible = match role {
        ActorRole::GateRunner => vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
            ContextClass::ToolObservation,
            ContextClass::AgentProgress,
        ],
        _ => vec![
            ContextClass::ImmutablePolicy,
            ContextClass::WorkspaceState,
            ContextClass::AgentProgress,
        ],
    };
    let required = match role {
        ActorRole::GateRunner => vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
        ],
        _ => vec![ContextClass::ImmutablePolicy, ContextClass::WorkspaceState],
    };

    ContextPolicy {
        required: ContextClassSet::from_canonical(required),
        visible: ContextClassSet::from_canonical(visible),
        contributable: ContextClassSet::from_canonical(vec![ContextClass::AgentProgress]),
        fresh_context: true,
        memory_visibility: MemoryVisibility::Excluded,
        reasoning_visibility: ReasoningVisibility::Excluded,
        allow_producer_ancestry: false,
        presentation: PresentationProfile::new(PresentationStyle::Restricted),
    }
}
fn base_required() -> ContextClassSet {
    ContextClassSet::from_canonical(vec![
        ContextClass::ImmutablePolicy,
        ContextClass::AcceptanceSpecification,
        ContextClass::ActiveUserRequest,
        ContextClass::RepositorySource,
        ContextClass::WorkspaceState,
    ])
}
fn all_classes() -> Vec<ContextClass> {
    vec![
        ContextClass::ImmutablePolicy,
        ContextClass::AcceptanceSpecification,
        ContextClass::ActiveUserRequest,
        ContextClass::RepositoryInstructions,
        ContextClass::RepositorySource,
        ContextClass::CandidateDiff,
        ContextClass::WorkspaceState,
        ContextClass::GateEvidence,
        ContextClass::ToolObservation,
        ContextClass::MemoryEvidence,
        ContextClass::PriorFinding,
        ContextClass::FindingResolution,
        ContextClass::AgentProgress,
        ContextClass::HiddenReasoning,
    ]
}
