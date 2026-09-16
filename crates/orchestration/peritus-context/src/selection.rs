//! Deterministic required-first selection with atomic optional closure admission.

use crate::{ContextError, ContextErrorKind, TokenBudget};
use peritus_role::RoleProfile;
use vstd::prelude::*;

verus! {

mod certificate;
mod closure;
mod ordering;
mod outcome;
mod plan;

pub use certificate::selection_result_is_exact;
pub use plan::select_context;
#[cfg(verus_only)]
pub use outcome::selection_success_matches;

/// Pure selection inputs and explicit bounds.
#[derive(Debug, Eq, PartialEq)]
pub struct SelectionPolicy {
    role_profile: RoleProfile,
    token_budget: TokenBudget,
    max_selected_nodes: usize,
    max_selected_bytes: usize,
}

impl SelectionPolicy {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_is_valid() }

    /// Logical view of the exact role and visibility profile.
    pub closed spec fn spec_role_profile(&self) -> RoleProfile { self.role_profile }

    /// Logical view of the exact token budget.
    pub closed spec fn spec_token_budget(&self) -> TokenBudget { self.token_budget }

    /// Logical view of the selected-node ceiling.
    pub closed spec fn spec_max_selected_nodes(&self) -> nat {
        self.max_selected_nodes as nat
    }

    /// Logical view of the selected-content byte ceiling.
    pub closed spec fn spec_max_selected_bytes(&self) -> nat {
        self.max_selected_bytes as nat
    }

    /// Intrinsic validity retained by every constructed policy.
    pub open spec fn spec_is_valid(&self) -> bool {
        &&& self.spec_token_budget().spec_is_valid()
        &&& self.spec_max_selected_nodes() > 0
        &&& self.spec_max_selected_bytes() > 0
    }

    /// Creates a selection policy with nonzero selected-node and byte limits.
    ///
    /// # Errors
    ///
    /// Returns [`ContextErrorKind::InvalidSelectionPolicy`] for a zero limit.
    pub fn new(
        role_profile: RoleProfile,
        token_budget: TokenBudget,
        max_selected_nodes: usize,
        max_selected_bytes: usize,
    ) -> (result: Result<Self, ContextError>)
        ensures match result {
            Ok(policy) => {
                &&& policy.spec_role_profile() == role_profile
                &&& policy.spec_token_budget() == token_budget
                &&& policy.spec_max_selected_nodes() == max_selected_nodes as nat
                &&& policy.spec_max_selected_bytes() == max_selected_bytes as nat
                &&& max_selected_nodes > 0
                &&& max_selected_bytes > 0
                &&& policy.spec_is_valid()
            }
            Err(error) => error.spec_is_plain(ContextErrorKind::InvalidSelectionPolicy)
                && (max_selected_nodes == 0 || max_selected_bytes == 0),
        },
    {
        if max_selected_nodes == 0 || max_selected_bytes == 0 {
            Err(ContextError::plain(ContextErrorKind::InvalidSelectionPolicy))
        } else {
            proof { use_type_invariant(&token_budget); }
            Ok(Self {
                role_profile,
                token_budget,
                max_selected_nodes,
                max_selected_bytes,
            })
        }
    }

    /// Returns the selected role profile.
    #[must_use]
    pub const fn role_profile(&self) -> (result: &RoleProfile)
        ensures *result == self.spec_role_profile(),
    {
        &self.role_profile
    }
    /// Returns the token budget.
    #[must_use]
    pub const fn token_budget(&self) -> (result: TokenBudget)
        ensures result == self.spec_token_budget(),
    {
        self.token_budget
    }
    /// Returns the selected-node bound.
    #[must_use]
    pub const fn max_selected_nodes(&self) -> (result: usize)
        ensures result as nat == self.spec_max_selected_nodes(),
    {
        self.max_selected_nodes
    }
    /// Returns the selected-byte bound.
    #[must_use]
    pub const fn max_selected_bytes(&self) -> (result: usize)
        ensures result as nat == self.spec_max_selected_bytes(),
    {
        self.max_selected_bytes
    }
}

impl Clone for SelectionPolicy {
    fn clone(&self) -> (result: Self)
        ensures
            RoleProfile::clone_equivalent(
                &self.spec_role_profile(),
                &result.spec_role_profile(),
            ),
            result.spec_token_budget() == self.spec_token_budget(),
            result.spec_max_selected_nodes() == self.spec_max_selected_nodes(),
            result.spec_max_selected_bytes() == self.spec_max_selected_bytes(),
            result.spec_is_valid(),
    {
        proof { use_type_invariant(self); }
        Self {
            role_profile: self.role_profile.clone(),
            token_budget: self.token_budget,
            max_selected_nodes: self.max_selected_nodes,
            max_selected_bytes: self.max_selected_bytes,
        }
    }
}

} // verus!
