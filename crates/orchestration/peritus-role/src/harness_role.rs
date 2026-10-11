//! Explicit harness roles and their canonical B1 identities.

use peritus_policy::ActorRole;

#[doc = " Agent roles that directly participate in the production development loop."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HarnessRole {
    #[doc = " Produces candidate changes."]
    Writer,
    #[doc = " Performs fresh-context read-only review."]
    Reviewer,
    #[doc = " Resolves current-review findings."]
    Fixer,
    #[doc = " Evaluates a candidate against isolated definitions and datasets."]
    Evaluator,
    #[doc = " Proposes and evaluates harness evolution candidates."]
    Evolver,
}
impl HarnessRole {
    #[doc = " Returns the canonical B1 security role. This mapping cannot widen authority."]
    #[must_use]
    pub const fn actor_role(self) -> ActorRole {
        match self {
            Self::Writer => ActorRole::Writer,
            Self::Reviewer => ActorRole::Reviewer,
            Self::Fixer => ActorRole::Fixer,
            Self::Evaluator => ActorRole::Evaluator,
            Self::Evolver => ActorRole::EvolutionAgent,
        }
    }
    #[doc = " Returns the harness role represented by a canonical B1 role, when applicable."]
    #[must_use]
    pub const fn from_actor_role(role: ActorRole) -> Option<Self> {
        match role {
            ActorRole::Writer => Some(Self::Writer),
            ActorRole::Reviewer => Some(Self::Reviewer),
            ActorRole::Fixer => Some(Self::Fixer),
            ActorRole::Evaluator => Some(Self::Evaluator),
            ActorRole::EvolutionAgent => Some(Self::Evolver),
            _ => None,
        }
    }
}
