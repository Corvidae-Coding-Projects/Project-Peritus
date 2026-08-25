//! Authority classes are independent from the source provenance label.

use vstd::prelude::*;

verus! {

/// Authority attached to content without interpreting the content text.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthorityClass {
    /// Non-overridable system policy.
    SystemPolicy,
    /// Application-level policy below system policy.
    ApplicationPolicy,
    /// Immutable acceptance criteria and specifications.
    AcceptanceSpecification,
    /// The active user's explicit instruction.
    UserInstruction,
    /// Evidence or derived text that carries no instruction authority.
    NonAuthoritative,
}

impl AuthorityClass {
    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::SystemPolicy => 5,
            Self::ApplicationPolicy => 4,
            Self::AcceptanceSpecification => 3,
            Self::UserInstruction => 2,
            Self::NonAuthoritative => 1,
        }
    }
}

} // verus!
