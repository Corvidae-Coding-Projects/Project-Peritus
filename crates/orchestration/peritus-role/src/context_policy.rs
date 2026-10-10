//! Immutable role-specific context policies.

use crate::{
    CapabilityView, ContextClass, ContextClassSet, HarnessRole, PresentationProfile,
    PresentationStyle,
};
use peritus_policy::ActorRole;

mod tables;
use tables::policy_for;

#[doc = " Whether scoped derived memory may be selected for the role."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MemoryVisibility {
    #[doc = " Memory is excluded from the role context."]
    Excluded,
    #[doc = " Only evidence-backed, active, unquarantined memory is eligible."]
    EvidenceBacked,
}
#[doc = " Which hidden model reasoning is eligible for the role context."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReasoningVisibility {
    #[doc = " Hidden reasoning is excluded."]
    Excluded,
    #[doc = " Only reasoning from the same actor/context lineage is eligible."]
    SameLineageOnly,
}
#[doc = " Complete immutable context policy for a B1 role."]
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
    #[doc = " Returns visible context classes."]
    #[must_use]
    pub const fn visible(&self) -> &ContextClassSet {
        &self.visible
    }
    #[doc = " Returns context classes the role may contribute as non-authoritative data."]
    #[must_use]
    pub const fn contributable(&self) -> &ContextClassSet {
        &self.contributable
    }
    #[doc = " Returns classes that a complete role context requires."]
    #[must_use]
    pub const fn required(&self) -> &ContextClassSet {
        &self.required
    }
    #[doc = " Whether the context must start without inherited model conversation state."]
    #[must_use]
    pub const fn requires_fresh_context(&self) -> bool {
        self.fresh_context
    }
    #[doc = " Returns the memory visibility rule."]
    #[must_use]
    pub const fn memory_visibility(&self) -> MemoryVisibility {
        self.memory_visibility
    }
    #[doc = " Returns the hidden-reasoning visibility rule."]
    #[must_use]
    pub const fn reasoning_visibility(&self) -> ReasoningVisibility {
        self.reasoning_visibility
    }
    #[doc = " Whether causal ancestry from the producing context may be included."]
    #[must_use]
    pub const fn allows_producer_ancestry(&self) -> bool {
        self.allow_producer_ancestry
    }
    #[doc = " Returns provider-neutral presentation policy."]
    #[must_use]
    pub const fn presentation(&self) -> PresentationProfile {
        self.presentation
    }
}
#[doc = " One canonical B1 role with its complete C6 context and capability projections."]
#[derive(Debug, Eq, PartialEq)]
pub struct RoleProfile {
    actor_role: ActorRole,
    harness_role: Option<HarnessRole>,
    context: ContextPolicy,
    capabilities: CapabilityView,
}
impl RoleProfile {
    #[doc = " Builds the deterministic profile for any canonical B1 role."]
    #[must_use]
    pub fn for_actor_role(actor_role: ActorRole) -> Self {
        let harness_role = HarnessRole::from_actor_role(actor_role);
        let context = policy_for(actor_role);
        let capabilities = CapabilityView::for_role(actor_role);
        Self { actor_role, harness_role, context, capabilities }
    }
    #[doc = " Builds the deterministic profile for a harness role."]
    #[must_use]
    pub fn for_harness_role(role: HarnessRole) -> Self {
        Self::for_actor_role(role.actor_role())
    }
    #[doc = " Returns the canonical B1 role."]
    #[must_use]
    pub const fn actor_role(&self) -> ActorRole {
        self.actor_role
    }
    #[doc = " Returns the direct harness role, if this is an agent-loop profile."]
    #[must_use]
    pub const fn harness_role(&self) -> Option<HarnessRole> {
        self.harness_role
    }
    #[doc = " Returns the immutable context policy."]
    #[must_use]
    pub const fn context(&self) -> &ContextPolicy {
        &self.context
    }
    #[doc = " Returns the non-widening capability view."]
    #[must_use]
    pub const fn capabilities(&self) -> &CapabilityView {
        &self.capabilities
    }
}
impl Clone for ContextPolicy {
    fn clone(&self) -> Self {
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
    fn clone(&self) -> Self {
        Self {
            actor_role: self.actor_role,
            harness_role: self.harness_role,
            context: self.context.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}
