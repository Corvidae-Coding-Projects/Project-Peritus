//! Immutable role-specific context policies.

use crate::{
    CapabilityView, ContextClass, ContextClassSet, HarnessRole, PresentationProfile,
    PresentationStyle,
};
use peritus_policy::ActorRole;
use vstd::prelude::*;

#[cfg(verus_only)]
mod model;
mod tables;
use tables::policy_for;

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
#[derive(Debug, Eq, PartialEq)]
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
    /// Exact stored visible.
    pub closed spec fn spec_visible(&self) -> ContextClassSet { self.visible }
    /// Exact stored contributable.
    pub closed spec fn spec_contributable(&self) -> ContextClassSet { self.contributable }
    /// Exact stored required.
    pub closed spec fn spec_required(&self) -> ContextClassSet { self.required }
    /// Exact stored fresh context.
    pub closed spec fn spec_fresh_context(&self) -> bool { self.fresh_context }
    /// Exact stored memory visibility.
    pub closed spec fn spec_memory_visibility(&self) -> MemoryVisibility { self.memory_visibility }
    /// Exact stored reasoning visibility.
    pub closed spec fn spec_reasoning_visibility(&self) -> ReasoningVisibility { self.reasoning_visibility }
    /// Exact stored allow producer ancestry.
    pub closed spec fn spec_allow_producer_ancestry(&self) -> bool { self.allow_producer_ancestry }
    /// Exact stored presentation.
    pub closed spec fn spec_presentation(&self) -> PresentationProfile { self.presentation }
    /// Complete deterministic context policy for a supplied canonical role.
    pub open spec fn spec_for_role(&self, role: ActorRole) -> bool {
        &&& self.spec_visible().spec_values() == model::visible(role)
        &&& self.spec_contributable().spec_values() == model::contributable(role)
        &&& self.spec_required().spec_values() == model::required(role)
        &&& self.spec_fresh_context() == !matches!(role, ActorRole::Writer | ActorRole::Fixer)
        &&& self.spec_memory_visibility() == if matches!(role,
            ActorRole::Writer | ActorRole::Fixer | ActorRole::Evaluator | ActorRole::EvolutionAgent) {
            MemoryVisibility::EvidenceBacked } else { MemoryVisibility::Excluded }
        &&& self.spec_reasoning_visibility() == if matches!(role,
            ActorRole::Writer | ActorRole::Fixer | ActorRole::EvolutionAgent) {
            ReasoningVisibility::SameLineageOnly } else { ReasoningVisibility::Excluded }
        &&& self.spec_allow_producer_ancestry() == matches!(role,
            ActorRole::Writer | ActorRole::Fixer | ActorRole::EvolutionAgent)
        &&& self.spec_presentation().spec_for_style(model::style(role))
    }

    /// Complete semantic equality of context-policy fields.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        &&& left.spec_visible().spec_values() == right.spec_visible().spec_values()
        &&& left.spec_contributable().spec_values() == right.spec_contributable().spec_values()
        &&& left.spec_required().spec_values() == right.spec_required().spec_values()
        &&& left.spec_fresh_context() == right.spec_fresh_context()
        &&& left.spec_memory_visibility() == right.spec_memory_visibility()
        &&& left.spec_reasoning_visibility() == right.spec_reasoning_visibility()
        &&& left.spec_allow_producer_ancestry() == right.spec_allow_producer_ancestry()
        &&& left.spec_presentation() == right.spec_presentation()
    }

    /// Returns visible context classes.
    #[must_use]
    pub const fn visible(&self) -> (value: &ContextClassSet)
        ensures *value == self.spec_visible(),
    { &self.visible }

    /// Returns context classes the role may contribute as non-authoritative data.
    #[must_use]
    pub const fn contributable(&self) -> (value: &ContextClassSet)
        ensures *value == self.spec_contributable(),
    { &self.contributable }

    /// Returns classes that a complete role context requires.
    #[must_use]
    pub const fn required(&self) -> (value: &ContextClassSet)
        ensures *value == self.spec_required(),
    { &self.required }

    /// Whether the context must start without inherited model conversation state.
    #[must_use]
    pub const fn requires_fresh_context(&self) -> (value: bool)
        ensures value == self.spec_fresh_context(),
    { self.fresh_context }

    /// Returns the memory visibility rule.
    #[must_use]
    pub const fn memory_visibility(&self) -> (value: MemoryVisibility)
        ensures value == self.spec_memory_visibility(),
    { self.memory_visibility }

    /// Returns the hidden-reasoning visibility rule.
    #[must_use]
    pub const fn reasoning_visibility(&self) -> (value: ReasoningVisibility)
        ensures value == self.spec_reasoning_visibility(),
    { self.reasoning_visibility }

    /// Whether causal ancestry from the producing context may be included.
    #[must_use]
    pub const fn allows_producer_ancestry(&self) -> (value: bool)
        ensures value == self.spec_allow_producer_ancestry(),
    { self.allow_producer_ancestry }

    /// Returns provider-neutral presentation policy.
    #[must_use]
    pub const fn presentation(&self) -> (value: PresentationProfile)
        ensures value == self.spec_presentation(),
    { self.presentation }
}

/// One canonical B1 role with its complete C6 context and capability projections.
#[derive(Debug, Eq, PartialEq)]
pub struct RoleProfile {
    actor_role: ActorRole,
    harness_role: Option<HarnessRole>,
    context: ContextPolicy,
    capabilities: CapabilityView,
}

impl RoleProfile {
    /// Exact role identity stored in the profile.
    pub closed spec fn spec_actor_role(&self) -> ActorRole { self.actor_role }
    /// Exact optional agent-loop role identity.
    pub closed spec fn spec_harness_role(&self) -> Option<HarnessRole> { self.harness_role }
    /// Exact stored context policy.
    pub closed spec fn spec_context(&self) -> ContextPolicy { self.context }
    /// Complete deterministic profile for a supplied canonical role.
    pub open spec fn spec_for_role(&self, role: ActorRole) -> bool {
        self.spec_actor_role() == role
            && self.spec_harness_role() == HarnessRole::spec_from_actor_role(role)
            && self.spec_context().spec_for_role(role)
            && self.spec_capabilities().spec_role() == role
            && self.spec_capabilities().spec_operations() == CapabilityView::spec_role_operations(role)
            && self.spec_capabilities().spec_is_narrow()
    }
    /// Complete semantic equality of every role-profile field.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_actor_role() == right.spec_actor_role()
            && left.spec_harness_role() == right.spec_harness_role()
            && ContextPolicy::clone_equivalent(&left.spec_context(), &right.spec_context())
            && CapabilityView::clone_equivalent(&left.spec_capabilities(), &right.spec_capabilities())
    }

    /// Returns the exact capability view used by specifications.
    pub closed spec fn spec_capabilities(&self) -> CapabilityView { self.capabilities }

    /// Builds the deterministic profile for any canonical B1 role.
    #[must_use]
    pub fn for_actor_role(actor_role: ActorRole) -> (profile: Self)
        ensures profile.spec_for_role(actor_role),
    {
        let harness_role = HarnessRole::from_actor_role(actor_role);
        let context = policy_for(actor_role);
        let capabilities = CapabilityView::for_role(actor_role);
        Self { actor_role, harness_role, context, capabilities }
    }

    /// Builds the deterministic profile for a harness role.
    #[must_use]
    pub fn for_harness_role(role: HarnessRole) -> (profile: Self)
        ensures profile.spec_for_role(role.spec_actor_role()), profile.spec_harness_role() == Some(role),
    {
        Self::for_actor_role(role.actor_role())
    }

    /// Returns the canonical B1 role.
    #[must_use]
    pub const fn actor_role(&self) -> (value: ActorRole)
        ensures value == self.spec_actor_role(),
    { self.actor_role }

    /// Returns the direct harness role, if this is an agent-loop profile.
    #[must_use]
    pub const fn harness_role(&self) -> (value: Option<HarnessRole>)
        ensures value == self.spec_harness_role(),
    { self.harness_role }

    /// Returns the immutable context policy.
    #[must_use]
    pub const fn context(&self) -> (value: &ContextPolicy)
        ensures *value == self.spec_context(),
    { &self.context }

    /// Returns the non-widening capability view.
    #[must_use]
    pub const fn capabilities(&self) -> (value: &CapabilityView)
        ensures *value == self.spec_capabilities(),
    { &self.capabilities }
}


impl Clone for ContextPolicy {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            visible: self.visible.clone(),
            contributable: self.contributable.clone(),
            required: self.required.clone(),
            fresh_context: self.fresh_context,
            memory_visibility: self.memory_visibility,
            reasoning_visibility: self.reasoning_visibility,
            allow_producer_ancestry: self.allow_producer_ancestry,
            presentation: self.presentation,
        }
    }
}

impl Clone for RoleProfile {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            actor_role: self.actor_role,
            harness_role: self.harness_role,
            context: self.context.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}

} // verus!
