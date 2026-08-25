//! Immutable deterministic selection plans and omission explanations.

use crate::{ContextNodeId, ContextPlanId, TokenAccounting};
use peritus_role::RoleProfile;
use vstd::prelude::*;

verus! {

/// Why a node entered the selected dependency closure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SelectionReason {
    /// An explicitly required root.
    RequiredRoot,
    /// A dependency of a required root.
    RequiredDependency,
    /// An admitted optional ranked root.
    OptionalRoot,
    /// A newly admitted dependency of an optional root.
    OptionalDependency,
}

/// One selected node and its explainable admission reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectedContext {
    node_id: ContextNodeId,
    reason: SelectionReason,
}

impl SelectedContext {
    pub(crate) const fn new(node_id: ContextNodeId, reason: SelectionReason) -> Self {
        Self { node_id, reason }
    }

    /// Returns the selected node identity.
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId { self.node_id }
    /// Returns why this node was selected.
    #[must_use]
    pub const fn reason(self) -> SelectionReason { self.reason }
}

/// Normal reason an optional root and its entire closure were not admitted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OmissionReason {
    /// A dependency was hidden from the selected role.
    HiddenDependency,
    /// The complete new closure exceeded remaining input tokens.
    TokenBudget,
    /// The complete new closure exceeded the selected-node limit.
    NodeLimit,
    /// The complete new closure exceeded the selected-byte limit.
    ByteLimit,
}

/// Explainable atomic omission of one ranked optional root.
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

    /// Returns the omitted optional root.
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId { self.node_id }
    /// Returns the normal omission reason.
    #[must_use]
    pub const fn reason(self) -> OmissionReason { self.reason }
    /// Returns the first canonical hidden dependency, when applicable.
    #[must_use]
    pub const fn blocking_dependency(self) -> Option<ContextNodeId> {
        self.blocking_dependency
    }
    /// Returns the new closure's token estimate, if it was fully visible.
    #[must_use]
    pub const fn required_tokens(self) -> u64 { self.required_tokens }
}

/// Complete immutable outcome of deterministic context selection.
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

    /// Returns the caller-bound immutable plan ID.
    #[must_use]
    pub const fn id(&self) -> ContextPlanId { self.id }
    /// Returns the role whose visibility policy was applied.
    #[must_use]
    pub const fn role_profile(&self) -> &RoleProfile { &self.role_profile }
    /// Borrows selected entries in deterministic render precedence.
    #[must_use]
    pub const fn selected(&self) -> &[SelectedContext] { self.selected.as_slice() }
    /// Borrows optional-root omissions in deterministic ranking order.
    #[must_use]
    pub const fn omitted(&self) -> &[OmittedContext] { self.omitted.as_slice() }
    /// Returns exact checked token accounting.
    #[must_use]
    pub const fn accounting(&self) -> TokenAccounting { self.accounting }
    /// Returns the exact selected content-byte total.
    #[must_use]
    pub const fn selected_bytes(&self) -> usize { self.selected_bytes }
    /// Returns whether an identity is selected.
    #[must_use]
    pub fn contains(&self, id: ContextNodeId) -> bool {
        let mut index = 0;
        while index < self.selected.len()
            decreases self.selected.len() - index,
        {
            if self.selected[index].node_id == id {
                return true;
            }
            index += 1;
        }
        false
    }
}

} // verus!
