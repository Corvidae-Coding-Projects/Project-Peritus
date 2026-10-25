//! Authenticated restriction-tier ordering and canonical lower layers.

use crate::{CanonicalCollection, PolicyError, RestrictionRule};
use core::cmp::Ordering;
use peritus_types::RevisionTuple;

#[doc = " Authenticated policy tier ordered from highest to lowest authority."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyTier {
    #[doc = " System or organization policy."]
    System,
    #[doc = " Authenticated user policy."]
    User,
    #[doc = " Committed project policy."]
    Project,
    #[doc = " Immutable run acceptance-contract restrictions."]
    Run,
    #[doc = " Session override bounded by every higher tier."]
    Session,
    #[doc = " Role or harness preference restrictions."]
    RoleHarness,
}
impl PolicyTier {
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::System => 0,
            Self::User => 1,
            Self::Project => 2,
            Self::Run => 3,
            Self::Session => 4,
            Self::RoleHarness => 5,
        }
    }
}
#[doc = " One complete lower restriction layer. An empty layer is neutral."]
#[derive(Debug, Eq, PartialEq)]
pub struct RestrictionLayer {
    tier: PolicyTier,
    rules: Vec<RestrictionRule>,
}
impl RestrictionLayer {
    #[doc = " Creates a layer whose rules are in strict digest order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise duplicate or ordering failure."]
    pub fn new(tier: PolicyTier, rules: Vec<RestrictionRule>) -> Result<Self, PolicyError> {
        let mut index = 1;
        while index < rules.len() {
            match rules[index - 1].canonical_cmp(&rules[index]) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(PolicyError::duplicate_canonical_value(
                        CanonicalCollection::RestrictionRules,
                    ));
                }
                Ordering::Greater => {
                    return Err(PolicyError::non_canonical_order(
                        CanonicalCollection::RestrictionRules,
                    ));
                }
            }
            index += 1;
        }
        let layer = Self { tier, rules };
        Ok(layer)
    }
    #[doc = " Returns the authenticated policy tier."]
    #[must_use]
    pub const fn tier(&self) -> PolicyTier {
        self.tier
    }
    #[doc = " Borrows canonical restriction-only rules."]
    #[must_use]
    pub const fn rules(&self) -> &[RestrictionRule] {
        self.rules.as_slice()
    }
    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> Self {
        let mut rules: Vec<RestrictionRule> = Vec::new();
        let mut index = 0;
        while index < self.rules.len() {
            rules.push(self.rules[index].rebind_revision(revision));
            index += 1;
        }

        Self { tier: self.tier, rules }
    }
}
