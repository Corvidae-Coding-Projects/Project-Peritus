//! Production context-policy constructors and canonical tables.
use super::{ContextPolicy, ContextClassSet, ContextClass, MemoryVisibility,
    ReasoningVisibility, PresentationProfile, PresentationStyle};
use peritus_policy::ActorRole;
#[cfg(verus_only)]
use super::model;
use vstd::prelude::*;

verus! {
pub(super) fn policy_for(role: ActorRole) -> (policy: ContextPolicy)
    ensures policy.spec_for_role(role),
{
    match role {
        ActorRole::Writer => writer_policy(),
        ActorRole::Reviewer => reviewer_policy(),
        ActorRole::Fixer => fixer_policy(),
        ActorRole::Evaluator => evaluator_policy(),
        ActorRole::EvolutionAgent => evolver_policy(),
        _ => restricted_policy(role),
    }
}

fn writer_policy() -> (policy: ContextPolicy)
    ensures policy.spec_for_role(ActorRole::Writer),
{
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

fn reviewer_policy() -> (policy: ContextPolicy)
    ensures policy.spec_for_role(ActorRole::Reviewer),
{
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

fn fixer_policy() -> (policy: ContextPolicy)
    ensures policy.spec_for_role(ActorRole::Fixer),
{
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

fn evaluator_policy() -> (policy: ContextPolicy)
    ensures policy.spec_for_role(ActorRole::Evaluator),
{
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

fn evolver_policy() -> (policy: ContextPolicy)
    ensures policy.spec_for_role(ActorRole::EvolutionAgent),
{
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

fn restricted_policy(role: ActorRole) -> (policy: ContextPolicy)
    ensures !matches!(role, ActorRole::Writer | ActorRole::Reviewer | ActorRole::Fixer
        | ActorRole::Evaluator | ActorRole::EvolutionAgent) ==> policy.spec_for_role(role),
{
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
    assert(!matches!(role, ActorRole::Writer | ActorRole::Reviewer | ActorRole::Fixer
        | ActorRole::Evaluator | ActorRole::EvolutionAgent) ==> visible@ == model::visible(role));
    let required = match role {
        ActorRole::GateRunner => vec![
            ContextClass::ImmutablePolicy,
            ContextClass::AcceptanceSpecification,
            ContextClass::WorkspaceState,
            ContextClass::GateEvidence,
        ],
        _ => vec![ContextClass::ImmutablePolicy, ContextClass::WorkspaceState],
    };
    assert(!matches!(role, ActorRole::Writer | ActorRole::Reviewer | ActorRole::Fixer
        | ActorRole::Evaluator | ActorRole::EvolutionAgent) ==> required@ == model::required(role));
    let policy = ContextPolicy {
        required: ContextClassSet::from_canonical(required),
        visible: ContextClassSet::from_canonical(visible),
        contributable: ContextClassSet::from_canonical(vec![ContextClass::AgentProgress]),
        fresh_context: true,
        memory_visibility: MemoryVisibility::Excluded,
        reasoning_visibility: ReasoningVisibility::Excluded,
        allow_producer_ancestry: false,
        presentation: PresentationProfile::new(PresentationStyle::Restricted),
    };
    proof {
        if !matches!(role, ActorRole::Writer | ActorRole::Reviewer | ActorRole::Fixer
            | ActorRole::Evaluator | ActorRole::EvolutionAgent) {
            assert(policy.spec_visible().spec_values() == model::visible(role));
            assert(policy.spec_required().spec_values() == model::required(role));
            assert(policy.spec_contributable().spec_values() == model::contributable(role));
            assert(policy.spec_fresh_context());
            assert(policy.spec_memory_visibility() == MemoryVisibility::Excluded);
            assert(policy.spec_reasoning_visibility() == ReasoningVisibility::Excluded);
            assert(!policy.spec_allow_producer_ancestry());
            assert(policy.spec_presentation().spec_for_style(model::style(role)));
            assert(policy.spec_for_role(role));
        }
    }
    policy
}

fn base_required() -> (set: ContextClassSet)
    ensures set.spec_values() == model::required(ActorRole::Writer),
{
    ContextClassSet::from_canonical(vec![
        ContextClass::ImmutablePolicy,
        ContextClass::AcceptanceSpecification,
        ContextClass::ActiveUserRequest,
        ContextClass::RepositorySource,
        ContextClass::WorkspaceState,
    ])
}

fn all_classes() -> (classes: Vec<ContextClass>)
    ensures classes@ == model::visible(ActorRole::Writer),
{
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

} // verus!
