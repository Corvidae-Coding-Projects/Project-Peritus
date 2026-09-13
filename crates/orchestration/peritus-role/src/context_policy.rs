//! Immutable role-specific context policies.

use crate::{
    CapabilityView, ContextClass, ContextClassSet, HarnessRole, PresentationProfile,
    PresentationStyle,
};
use peritus_policy::ActorRole;
use vstd::prelude::*;

verus! {

/// Whether scoped derived memory may be selected for the role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryVisibility {
    /// Memory is excluded from the role context.
    Excluded,
    /// Only evidence-backed, active, unquarantined memory is eligible.
    EvidenceBacked,
}

/// Which hidden model reasoning is eligible for the role context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReasoningVisibility {
    /// Hidden reasoning is excluded.
    Excluded,
    /// Only reasoning from the same actor/context lineage is eligible.
    SameLineageOnly,
}

/// Complete immutable context policy for a B1 role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPolicy {
    visible: ContextClassSet,
    contributable: ContextClassSet,
    required: ContextClassSet,
    fresh_context: bool,
    memory_visibility: MemoryVisibility,
    reasoning_visibility: ReasoningVisibility,
    allow_producer_ancestry: bool,
    presentation: PresentationProfile,
}

impl ContextPolicy {
    /// Returns visible context classes.
    #[must_use]
    pub const fn visible(&self) -> &ContextClassSet { &self.visible }

    /// Returns context classes the role may contribute as non-authoritative data.
    #[must_use]
    pub const fn contributable(&self) -> &ContextClassSet { &self.contributable }

    /// Returns classes that a complete role context requires.
    #[must_use]
    pub const fn required(&self) -> &ContextClassSet { &self.required }

    /// Whether the context must start without inherited model conversation state.
    #[must_use]
    pub const fn requires_fresh_context(&self) -> bool { self.fresh_context }

    /// Returns the memory visibility rule.
    #[must_use]
    pub const fn memory_visibility(&self) -> MemoryVisibility { self.memory_visibility }

    /// Returns the hidden-reasoning visibility rule.
    #[must_use]
    pub const fn reasoning_visibility(&self) -> ReasoningVisibility { self.reasoning_visibility }

    /// Whether causal ancestry from the producing context may be included.
    #[must_use]
    pub const fn allows_producer_ancestry(&self) -> bool { self.allow_producer_ancestry }

    /// Returns provider-neutral presentation policy.
    #[must_use]
    pub const fn presentation(&self) -> PresentationProfile { self.presentation }
}

/// One canonical B1 role with its complete C6 context and capability projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleProfile {
    actor_role: ActorRole,
    harness_role: Option<HarnessRole>,
    context: ContextPolicy,
    capabilities: CapabilityView,
}

impl RoleProfile {
    /// Returns the exact capability view used by specifications.
    pub closed spec fn spec_capabilities(&self) -> CapabilityView { self.capabilities }

    /// Builds the deterministic profile for any canonical B1 role.
    #[must_use]
    pub fn for_actor_role(actor_role: ActorRole) -> Self {
        let harness_role = HarnessRole::from_actor_role(actor_role);
        let context = policy_for(actor_role);
        let capabilities = CapabilityView::for_role(actor_role);
        Self { actor_role, harness_role, context, capabilities }
    }

    /// Builds the deterministic profile for a harness role.
    #[must_use]
    pub fn for_harness_role(role: HarnessRole) -> Self {
        Self::for_actor_role(role.actor_role())
    }

    /// Returns the canonical B1 role.
    #[must_use]
    pub const fn actor_role(&self) -> ActorRole { self.actor_role }

    /// Returns the direct harness role, if this is an agent-loop profile.
    #[must_use]
    pub const fn harness_role(&self) -> Option<HarnessRole> { self.harness_role }

    /// Returns the immutable context policy.
    #[must_use]
    pub const fn context(&self) -> &ContextPolicy { &self.context }

    /// Returns the non-widening capability view.
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityView { &self.capabilities }
}

fn policy_for(role: ActorRole) -> ContextPolicy {
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

} // verus!
