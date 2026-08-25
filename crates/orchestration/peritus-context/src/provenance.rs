//! Provenance and its authority/trust compatibility ceiling.

use crate::{AuthorityClass, TrustClass};
use vstd::prelude::*;

verus! {

/// Origin of context content. Text never changes this label.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Provenance {
    /// Peritus system policy.
    System,
    /// Peritus application policy or immutable specification.
    Application,
    /// Active user input.
    User,
    /// Repository-controlled content.
    Repository,
    /// External or web content.
    External,
    /// Derived scoped memory.
    Memory,
    /// Tool-produced observation.
    Tool,
    /// Agent-produced content.
    Agent,
    /// Reviewer-produced content.
    Review,
    /// A validated compaction derivation.
    DerivedCompaction,
}

impl Provenance {
    /// Mathematical authority ceiling for one exact source class.
    pub open spec fn spec_permits_authority(self, authority: AuthorityClass) -> bool {
        match self {
            Self::System => true,
            Self::Application => !matches!(authority, AuthorityClass::SystemPolicy),
            Self::User => matches!(
                authority,
                AuthorityClass::UserInstruction | AuthorityClass::NonAuthoritative
            ),
            Self::Repository
            | Self::External
            | Self::Memory
            | Self::Tool
            | Self::Agent
            | Self::Review
            | Self::DerivedCompaction => {
                matches!(authority, AuthorityClass::NonAuthoritative)
            }
        }
    }

    /// Whether an authority label is compatible with this source.
    #[must_use]
    pub const fn permits_authority(self, authority: AuthorityClass) -> (result: bool)
        ensures result == self.spec_permits_authority(authority),
    {
        match self {
            Self::System => true,
            Self::Application => !matches!(authority, AuthorityClass::SystemPolicy),
            Self::User => matches!(
                authority,
                AuthorityClass::UserInstruction | AuthorityClass::NonAuthoritative
            ),
            Self::Repository
            | Self::External
            | Self::Memory
            | Self::Tool
            | Self::Agent
            | Self::Review
            | Self::DerivedCompaction => {
                matches!(authority, AuthorityClass::NonAuthoritative)
            }
        }
    }

    /// Mathematical trust ceiling for one exact source class.
    pub open spec fn spec_permits_trust(self, trust: TrustClass) -> bool {
        match self {
            Self::System | Self::Application | Self::User => true,
            Self::Repository | Self::Tool | Self::Agent | Self::Review => {
                !matches!(trust, TrustClass::Trusted)
            }
            Self::External | Self::Memory | Self::DerivedCompaction => {
                matches!(trust, TrustClass::Untrusted)
            }
        }
    }

    /// Whether a trust label is at or below this source's ceiling.
    #[must_use]
    pub const fn permits_trust(self, trust: TrustClass) -> (result: bool)
        ensures result == self.spec_permits_trust(trust),
    {
        match self {
            Self::System | Self::Application | Self::User => true,
            Self::Repository | Self::Tool | Self::Agent | Self::Review => {
                !matches!(trust, TrustClass::Trusted)
            }
            Self::External | Self::Memory | Self::DerivedCompaction => {
                matches!(trust, TrustClass::Untrusted)
            }
        }
    }

    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::System => 10,
            Self::Application => 9,
            Self::User => 8,
            Self::Repository => 7,
            Self::Review => 6,
            Self::Tool => 5,
            Self::Agent => 4,
            Self::Memory => 3,
            Self::DerivedCompaction => 2,
            Self::External => 1,
        }
    }
}

} // verus!
