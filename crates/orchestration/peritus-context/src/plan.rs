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
    /// Logical view of the exact selected node identity.
    pub closed spec fn spec_node_id(self) -> ContextNodeId { self.node_id }

    /// Logical view of the exact admission reason.
    pub closed spec fn spec_reason(self) -> SelectionReason { self.reason }

    pub(crate) const fn new(
        node_id: ContextNodeId,
        reason: SelectionReason,
    ) -> (result: Self)
        ensures
            result.spec_node_id() == node_id,
            result.spec_reason() == reason,
    {
        Self { node_id, reason }
    }

    /// Returns the selected node identity.
    #[must_use]
    pub const fn node_id(self) -> (result: ContextNodeId)
        ensures result == self.spec_node_id(),
    {
        self.node_id
    }
    /// Returns why this node was selected.
    #[must_use]
    pub const fn reason(self) -> (result: SelectionReason)
        ensures result == self.spec_reason(),
    {
        self.reason
    }
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
    /// Logical view of the omitted optional root identity.
    pub closed spec fn spec_node_id(self) -> ContextNodeId { self.node_id }

    /// Logical view of the exact omission reason.
    pub closed spec fn spec_reason(self) -> OmissionReason { self.reason }

    /// Logical view of the first hidden dependency, when that caused omission.
    pub closed spec fn spec_blocking_dependency(self) -> Option<ContextNodeId> {
        self.blocking_dependency
    }

    /// Logical view of the complete newly required token estimate.
    pub closed spec fn spec_required_tokens(self) -> u64 { self.required_tokens }

    pub(crate) const fn new(
        node_id: ContextNodeId,
        reason: OmissionReason,
        blocking_dependency: Option<ContextNodeId>,
        required_tokens: u64,
    ) -> (result: Self)
        ensures
            result.spec_node_id() == node_id,
            result.spec_reason() == reason,
            result.spec_blocking_dependency() == blocking_dependency,
            result.spec_required_tokens() == required_tokens,
    {
        Self { node_id, reason, blocking_dependency, required_tokens }
    }

    /// Returns the omitted optional root.
    #[must_use]
    pub const fn node_id(self) -> (result: ContextNodeId)
        ensures result == self.spec_node_id(),
    {
        self.node_id
    }
    /// Returns the normal omission reason.
    #[must_use]
    pub const fn reason(self) -> (result: OmissionReason)
        ensures result == self.spec_reason(),
    {
        self.reason
    }
    /// Returns the first canonical hidden dependency, when applicable.
    #[must_use]
    pub const fn blocking_dependency(self) -> (result: Option<ContextNodeId>)
        ensures result == self.spec_blocking_dependency(),
    {
        self.blocking_dependency
    }
    /// Returns the new closure's token estimate, if it was fully visible.
    #[must_use]
    pub const fn required_tokens(self) -> (result: u64)
        ensures result == self.spec_required_tokens(),
    {
        self.required_tokens
    }
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
    /// Logical view of the caller-bound plan identity.
    pub closed spec fn spec_id(&self) -> ContextPlanId { self.id }

    /// Logical view of the frozen role profile.
    pub closed spec fn spec_role_profile(&self) -> RoleProfile { self.role_profile }

    /// Logical view of selected entries in render order.
    pub closed spec fn spec_selected(&self) -> Seq<SelectedContext> { self.selected@ }

    /// Logical view of optional-root omissions in ranking order.
    pub closed spec fn spec_omitted(&self) -> Seq<OmittedContext> { self.omitted@ }

    /// Logical view of exact token accounting.
    pub closed spec fn spec_accounting(&self) -> TokenAccounting { self.accounting }

    /// Logical view of exact selected content bytes.
    pub closed spec fn spec_selected_bytes(&self) -> nat { self.selected_bytes as nat }

    /// Exact role, token reservation, and byte limits retained from selection.
    pub open spec fn spec_respects_policy(&self, policy: &crate::SelectionPolicy) -> bool {
        &&& RoleProfile::clone_equivalent(
            &policy.spec_role_profile(),
            &self.spec_role_profile(),
        )
        &&& self.spec_accounting().spec_context_window()
            == policy.spec_token_budget().spec_context_window()
        &&& self.spec_accounting().spec_reserved_output()
            == policy.spec_token_budget().spec_reserved_output()
        &&& self.spec_accounting().spec_reserved_protocol_overhead()
            == policy.spec_token_budget().spec_reserved_protocol_overhead()
        &&& self.spec_accounting().spec_usable_input()
            == policy.spec_token_budget().spec_usable_input()
        &&& self.spec_accounting().spec_is_bounded()
        &&& self.spec_selected_bytes() <= policy.spec_max_selected_bytes()
    }

    pub(crate) const fn new(
        id: ContextPlanId,
        role_profile: RoleProfile,
        selected: Vec<SelectedContext>,
        omitted: Vec<OmittedContext>,
        accounting: TokenAccounting,
        selected_bytes: usize,
    ) -> (result: Self)
        ensures
            result.spec_id() == id,
            result.spec_role_profile() == role_profile,
            result.spec_selected() == selected@,
            result.spec_omitted() == omitted@,
            result.spec_accounting() == accounting,
            result.spec_selected_bytes() == selected_bytes as nat,
    {
        Self { id, role_profile, selected, omitted, accounting, selected_bytes }
    }

    /// Returns the caller-bound immutable plan ID.
    #[must_use]
    pub const fn id(&self) -> (result: ContextPlanId)
        ensures result == self.spec_id(),
    {
        self.id
    }
    /// Returns the role whose visibility policy was applied.
    #[must_use]
    pub const fn role_profile(&self) -> (result: &RoleProfile)
        ensures *result == self.spec_role_profile(),
    {
        &self.role_profile
    }
    /// Borrows selected entries in deterministic render precedence.
    #[must_use]
    pub const fn selected(&self) -> (result: &[SelectedContext])
        ensures result@ == self.spec_selected(),
    {
        self.selected.as_slice()
    }
    /// Borrows optional-root omissions in deterministic ranking order.
    #[must_use]
    pub const fn omitted(&self) -> (result: &[OmittedContext])
        ensures result@ == self.spec_omitted(),
    {
        self.omitted.as_slice()
    }
    /// Returns exact checked token accounting.
    #[must_use]
    pub const fn accounting(&self) -> (result: TokenAccounting)
        ensures result == self.spec_accounting(),
    {
        self.accounting
    }
    /// Returns the exact selected content-byte total.
    #[must_use]
    pub const fn selected_bytes(&self) -> (result: usize)
        ensures result as nat == self.spec_selected_bytes(),
    {
        self.selected_bytes
    }
    /// Returns whether an identity is selected.
    pub open spec fn spec_contains(&self, id: ContextNodeId) -> bool {
        exists |index: int| 0 <= index < self.spec_selected().len()
            && self.spec_selected()[index].spec_node_id().spec_matches(&id)
    }

    /// Returns whether an identity is selected.
    #[must_use]
    pub fn contains(&self, id: ContextNodeId) -> (found: bool)
        ensures found == self.spec_contains(id),
    {
        proof {
            reveal(ContextPlan::spec_selected);
            reveal(ContextPlan::spec_contains);
            reveal(SelectedContext::spec_node_id);
        }
        let mut index = 0;
        while index < self.selected.len()
            invariant
                index <= self.selected@.len(),
                forall |prior: int| 0 <= prior < index ==>
                    !self.selected@[prior].spec_node_id().spec_matches(&id),
            decreases self.selected.len() - index,
        {
            if self.selected[index].node_id.matches(&id) {
                proof {
                    assert(exists |found_index: int| 0 <= found_index
                        < self.spec_selected().len()
                        && self.spec_selected()[found_index].spec_node_id().spec_matches(&id)) by {
                        assert(self.spec_selected()[index as int]
                            .spec_node_id().spec_matches(&id));
                    }
                }
                return true;
            }
            proof {
                assert(!self.selected@[index as int].spec_node_id().spec_matches(&id));
            }
            index += 1;
        }
        proof {
            assert(!exists |found_index: int| 0 <= found_index
                < self.spec_selected().len()
                && self.spec_selected()[found_index].spec_node_id().spec_matches(&id));
        }
        false
    }
}

} // verus!
