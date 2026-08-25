//! Deterministic required-first selection with atomic optional closure admission.

use crate::{ContextError, ContextErrorKind, TokenBudget};
use peritus_role::RoleProfile;
use vstd::prelude::*;

verus! {

mod closure;
mod ordering;
mod plan;

pub use plan::select_context;

/// Pure selection inputs and explicit bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionPolicy {
    role_profile: RoleProfile,
    token_budget: TokenBudget,
    max_selected_nodes: usize,
    max_selected_bytes: usize,
}

impl SelectionPolicy {
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
    ) -> Result<Self, ContextError> {
        if max_selected_nodes == 0 || max_selected_bytes == 0 {
            Err(ContextError::plain(ContextErrorKind::InvalidSelectionPolicy))
        } else {
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
    pub const fn role_profile(&self) -> &RoleProfile { &self.role_profile }
    /// Returns the token budget.
    #[must_use]
    pub const fn token_budget(&self) -> TokenBudget { self.token_budget }
    /// Returns the selected-node bound.
    #[must_use]
    pub const fn max_selected_nodes(&self) -> usize { self.max_selected_nodes }
    /// Returns the selected-byte bound.
    #[must_use]
    pub const fn max_selected_bytes(&self) -> usize { self.max_selected_bytes }
}

} // verus!
