//! Immutable deterministic selection plans and omission explanations.

use crate::{ContextNodeId, ContextPlanId, TokenAccounting};
use peritus_role::RoleProfile;

#[doc = " Why a node entered the selected dependency closure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SelectionReason {
    #[doc = " An explicitly required root."]
    RequiredRoot,
    #[doc = " A dependency of a required root."]
    RequiredDependency,
    #[doc = " An admitted optional ranked root."]
    OptionalRoot,
    #[doc = " A newly admitted dependency of an optional root."]
    OptionalDependency,
}
#[doc = " One selected node and its explainable admission reason."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedContext {
    node_id: ContextNodeId,
    reason: SelectionReason,
}
impl SelectedContext {
    pub(crate) const fn new(node_id: ContextNodeId, reason: SelectionReason) -> Self {
        Self { node_id, reason }
    }
    #[doc = " Returns the selected node identity."]
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId {
        self.node_id
    }
    #[doc = " Returns why this node was selected."]
    #[must_use]
    pub const fn reason(self) -> SelectionReason {
        self.reason
    }
}
#[doc = " Normal reason an optional root and its entire closure were not admitted."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OmissionReason {
    #[doc = " A dependency was hidden from the selected role."]
    HiddenDependency,
    #[doc = " The complete new closure exceeded remaining input tokens."]
    TokenBudget,
    #[doc = " The complete new closure exceeded the selected-node limit."]
    NodeLimit,
    #[doc = " The complete new closure exceeded the selected-byte limit."]
    ByteLimit,
}
#[doc = " Explainable atomic omission of one ranked optional root."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OmittedContext {
    node_id: ContextNodeId,
    reason: OmissionReason,
    blocking_dependency: Option<ContextNodeId>,
    required_tokens: u64,
}
impl OmittedContext {
    pub(crate) const fn new(
        node_id: ContextNodeId,
        reason: OmissionReason,
        blocking_dependency: Option<ContextNodeId>,
        required_tokens: u64,
    ) -> Self {
        Self { node_id, reason, blocking_dependency, required_tokens }
    }
    #[doc = " Returns the omitted optional root."]
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId {
        self.node_id
    }
    #[doc = " Returns the normal omission reason."]
    #[must_use]
    pub const fn reason(self) -> OmissionReason {
        self.reason
    }
    #[doc = " Returns the first canonical hidden dependency, when applicable."]
    #[must_use]
    pub const fn blocking_dependency(self) -> Option<ContextNodeId> {
        self.blocking_dependency
    }
    #[doc = " Returns the new closure\'s token estimate, if it was fully visible."]
    #[must_use]
    pub const fn required_tokens(self) -> u64 {
        self.required_tokens
    }
}
#[doc = " Complete immutable outcome of deterministic context selection."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPlan {
    id: ContextPlanId,
    role_profile: RoleProfile,
    selected: Vec<SelectedContext>,
    omitted: Vec<OmittedContext>,
    accounting: TokenAccounting,
    selected_bytes: usize,
}
impl ContextPlan {
    pub(crate) const fn new(
        id: ContextPlanId,
        role_profile: RoleProfile,
        selected: Vec<SelectedContext>,
        omitted: Vec<OmittedContext>,
        accounting: TokenAccounting,
        selected_bytes: usize,
    ) -> Self {
        Self { id, role_profile, selected, omitted, accounting, selected_bytes }
    }
    #[doc = " Returns the caller-bound immutable plan ID."]
    #[must_use]
    pub const fn id(&self) -> ContextPlanId {
        self.id
    }
    #[doc = " Returns the role whose visibility policy was applied."]
    #[must_use]
    pub const fn role_profile(&self) -> &RoleProfile {
        &self.role_profile
    }
    #[doc = " Borrows selected entries in deterministic render precedence."]
    #[must_use]
    pub const fn selected(&self) -> &[SelectedContext] {
        self.selected.as_slice()
    }
    #[doc = " Borrows optional-root omissions in deterministic ranking order."]
    #[must_use]
    pub const fn omitted(&self) -> &[OmittedContext] {
        self.omitted.as_slice()
    }
    #[doc = " Returns exact checked token accounting."]
    #[must_use]
    pub const fn accounting(&self) -> TokenAccounting {
        self.accounting
    }
    #[doc = " Returns the exact selected content-byte total."]
    #[must_use]
    pub const fn selected_bytes(&self) -> usize {
        self.selected_bytes
    }
    #[doc = " Returns whether an identity is selected."]
    #[must_use]
    pub fn contains(&self, id: ContextNodeId) -> bool {
        let mut index = 0;
        while index < self.selected.len() {
            if self.selected[index].node_id.matches(&id) {
                return true;
            }
            index += 1;
        }
        false
    }
}
