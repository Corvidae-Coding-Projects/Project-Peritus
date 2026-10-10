//! Provenance and its authority/trust compatibility ceiling.

use crate::{AuthorityClass, TrustClass};

#[doc = " Origin of context content. Text never changes this label."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Provenance {
    #[doc = " Peritus system policy."]
    System,
    #[doc = " Peritus application policy or immutable specification."]
    Application,
    #[doc = " Active user input."]
    User,
    #[doc = " Repository-controlled content."]
    Repository,
    #[doc = " External or web content."]
    External,
    #[doc = " Derived scoped memory."]
    Memory,
    #[doc = " Tool-produced observation."]
    Tool,
    #[doc = " Agent-produced content."]
    Agent,
    #[doc = " Reviewer-produced content."]
    Review,
    #[doc = " A validated compaction derivation."]
    DerivedCompaction,
}
impl Provenance {
    #[doc = " Whether an authority label is compatible with this source."]
    #[must_use]
    pub const fn permits_authority(self, authority: AuthorityClass) -> bool {
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
    #[doc = " Whether a trust label is at or below this source\'s ceiling."]
    #[must_use]
    pub const fn permits_trust(self, trust: TrustClass) -> bool {
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
