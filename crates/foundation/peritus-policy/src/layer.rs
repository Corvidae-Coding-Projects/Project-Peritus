//! Authenticated restriction-tier ordering and canonical lower layers.

use crate::{CanonicalCollection, PolicyError, RestrictionRule};
use core::cmp::Ordering;
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub open spec fn first_rule_order_error(
    rules: Seq<RestrictionRule>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases rules.len() - index,
{
    if index >= rules.len() {
        None
    } else {
        match rules[index as int - 1].spec_canonical_cmp(&rules[index as int]) {
            Ordering::Less => first_rule_order_error(rules, index + 1),
            Ordering::Equal => Some(crate::PolicyErrorKind::DuplicateCanonicalValue),
            Ordering::Greater => Some(crate::PolicyErrorKind::NonCanonicalOrder),
        }
    }
}

/// Authenticated policy tier ordered from highest to lowest authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyTier {
    /// System or organization policy.
    System,
    /// Authenticated user policy.
    User,
    /// Committed project policy.
    Project,
    /// Immutable run acceptance-contract restrictions.
    Run,
    /// Session override bounded by every higher tier.
    Session,
    /// Role or harness preference restrictions.
    RoleHarness,
}

impl PolicyTier {
    /// Returns the exact tier order used by amendment specifications.
    pub open spec fn spec_rank(self) -> int {
        match self {
            Self::System => 0,
            Self::User => 1,
            Self::Project => 2,
            Self::Run => 3,
            Self::Session => 4,
            Self::RoleHarness => 5,
        }
    }

    pub(crate) const fn rank(self) -> (rank: u8)
        ensures rank as int == self.spec_rank(),
    {
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

/// One complete lower restriction layer. An empty layer is neutral.
#[derive(Debug, Eq, PartialEq)]
pub struct RestrictionLayer {
    tier: PolicyTier,
    rules: Vec<RestrictionRule>,
}

impl RestrictionLayer {
    /// Returns the exact authenticated tier used by amendment specifications.
    pub closed spec fn spec_tier(&self) -> PolicyTier { self.tier }

    /// Returns whether this layer preserves every rule under an exact revision-only rebind.
    pub closed spec fn spec_is_revision_rebind_of(
        &self,
        original: &Self,
        revision: RevisionTuple,
    ) -> bool {
        self.spec_tier() == original.spec_tier()
            && self.spec_rules().len() == original.spec_rules().len()
            && forall |index: int| 0 <= index < self.spec_rules().len() ==>
                #[trigger] self.spec_rules()[index].spec_is_revision_rebind_of(
                    &original.spec_rules()[index],
                    revision,
                )
    }
    /// Returns the exact canonical rule sequence used by evaluation specifications.
    pub closed spec fn spec_rules(&self) -> Seq<RestrictionRule> { self.rules@ }
    /// Creates a layer whose rules are in strict digest order.
    ///
    /// # Errors
    ///
    /// Returns a precise duplicate or ordering failure.
    pub fn new(
        tier: PolicyTier,
        rules: Vec<RestrictionRule>,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(layer) => {
                    first_rule_order_error(rules@, 1).is_none()
                        && layer.spec_tier() == tier
                        && layer.spec_rules() == rules@
                }
                Err(error) => {
                    first_rule_order_error(rules@, 1) == Some(error.spec_kind())
                        && error.spec_collection()
                            == Some(CanonicalCollection::RestrictionRules)
                        && error.spec_dimension().is_none()
                }
            },
    {
        let mut index = 1;
        while index < rules.len()
            invariant
                (rules.len() == 0 && index == 1)
                    || 1 <= index <= rules.len(),
                first_rule_order_error(rules@, 1)
                    == first_rule_order_error(rules@, index as nat),
            decreases rules.len() - index,
        {
            match rules[index - 1].canonical_cmp(&rules[index]) {
                Ordering::Less => {},
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
        reveal(RestrictionLayer::spec_tier);
        reveal(RestrictionLayer::spec_rules);
        Ok(layer)
    }

    /// Returns the authenticated policy tier.
    #[must_use]
    pub const fn tier(&self) -> (tier: PolicyTier)
        ensures tier == self.spec_tier(),
    {
        self.tier
    }

    /// Borrows canonical restriction-only rules.
    #[must_use]
    pub const fn rules(&self) -> (rules: &[RestrictionRule])
        ensures rules@ == self.spec_rules(),
    { self.rules.as_slice() }

    pub(crate) fn rebind_revision(&self, revision: RevisionTuple) -> (rebound: Self)
        ensures rebound.spec_is_revision_rebind_of(self, revision),
    {
        let mut rules: Vec<RestrictionRule> = Vec::new();
        let mut index = 0;
        while index < self.rules.len()
            invariant
                0 <= index <= self.rules.len(),
                rules@.len() == index,
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] rules@[prior].spec_is_revision_rebind_of(
                        &self.rules@[prior],
                        revision,
                    ),
            decreases self.rules.len() - index,
        {
            rules.push(self.rules[index].rebind_revision(revision));
            index += 1;
        }
        let rebound = Self { tier: self.tier, rules };
        reveal(RestrictionLayer::spec_is_revision_rebind_of);
        rebound
    }
}

} // verus!
