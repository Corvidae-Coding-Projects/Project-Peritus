//! Restriction-only policy rules and approval conjunction values.

use crate::{
    scope_validation::validate_role_values, ActorRole, IndependenceSet, PolicyError, ValidityWindow,
};
#[cfg(verus_only)]
use crate::{scope_validation::role_validation_error, CanonicalCollection};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Minimum authenticated authority tier required by an escalation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorityTier {
    /// Authenticated project authority.
    Project,
    /// Authenticated user authority.
    User,
    /// Authenticated organization authority.
    Organization,
    /// Protected system authority.
    System,
}

impl AuthorityTier {
    /// Returns the exact authority-tier rank used by specifications.
    pub open spec fn spec_rank(self) -> int {
        match self {
            Self::Project => 0,
            Self::User => 1,
            Self::Organization => 2,
            Self::System => 3,
        }
    }

    /// Returns whether this authenticated tier satisfies an exact minimum authority tier.
    pub open spec fn spec_at_least(self, required: Self) -> bool {
        self.spec_rank() >= required.spec_rank()
    }

    /// Checks whether this authenticated tier satisfies an exact minimum authority tier.
    #[must_use]
    pub const fn at_least(self, required: Self) -> (result: bool)
        ensures result == self.spec_at_least(required),
    {
        self.rank() >= required.rank()
    }

    pub(crate) const fn rank(self) -> (rank: u8)
        ensures rank as int == self.spec_rank(),
    {
        match self {
            Self::Project => 0,
            Self::User => 1,
            Self::Organization => 2,
            Self::System => 3,
        }
    }

    pub(crate) const fn maximum(self, other: Self) -> (maximum: Self)
        ensures maximum == crate::approval_model::maximum_authority_tier(self, other),
    {
        if self.rank() >= other.rank() { self } else { other }
    }
}

/// One approval rule's tier, role, independence, and validity constraints.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalRequirement {
    minimum_tier: AuthorityTier,
    approver_roles: Vec<ActorRole>,
    independence: IndependenceSet,
    validity: ValidityWindow,
}

impl ApprovalRequirement {
    /// Returns whether two requirements contain the exact same approval constraints.
    pub closed spec fn spec_same_as(&self, other: &Self) -> bool {
        self.spec_minimum_tier() == other.spec_minimum_tier()
            && self.spec_approver_roles() == other.spec_approver_roles()
            && self.spec_independence() == other.spec_independence()
            && self.spec_validity() == other.spec_validity()
    }
    /// Returns the exact minimum authority tier used by specifications.
    pub closed spec fn spec_minimum_tier(&self) -> AuthorityTier { self.minimum_tier }

    /// Returns the exact canonical approver-role sequence used by specifications.
    pub closed spec fn spec_approver_roles(&self) -> Seq<ActorRole> { self.approver_roles@ }

    /// Returns the exact independence sequence used by specifications.
    pub closed spec fn spec_independence(&self) -> Seq<crate::IndependenceRequirement> {
        self.independence.spec_values()
    }

    /// Returns the exact approval validity used by specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }

    /// Creates a requirement with canonical nonempty approver roles.
    ///
    /// # Errors
    ///
    /// Returns a precise role-set validation failure.
    pub fn new(
        minimum_tier: AuthorityTier,
        approver_roles: Vec<ActorRole>,
        independence: IndependenceSet,
        validity: ValidityWindow,
    ) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(requirement) => {
                    role_validation_error(approver_roles@).is_none()
                        && requirement.spec_minimum_tier() == minimum_tier
                        && requirement.spec_approver_roles() == approver_roles@
                        && requirement.spec_independence() == independence.spec_values()
                        && requirement.spec_validity() == validity
                }
                Err(error) => {
                    role_validation_error(approver_roles@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Roles)
                        && error.spec_dimension().is_none()
                }
            },
    {
        validate_role_values(approver_roles.as_slice())?;
        let requirement = Self { minimum_tier, approver_roles, independence, validity };
        reveal(ApprovalRequirement::spec_minimum_tier);
        reveal(ApprovalRequirement::spec_approver_roles);
        reveal(ApprovalRequirement::spec_independence);
        reveal(ApprovalRequirement::spec_validity);
        Ok(requirement)
    }

    /// Returns the minimum authority tier.
    #[must_use]
    pub const fn minimum_tier(&self) -> (tier: AuthorityTier)
        ensures tier == self.spec_minimum_tier(),
    { self.minimum_tier }

    /// Borrows canonical allowed approver roles.
    #[must_use]
    pub const fn approver_roles(&self) -> (roles: &[ActorRole])
        ensures roles@ == self.spec_approver_roles(),
    { self.approver_roles.as_slice() }

    /// Returns the independence conjunction.
    #[must_use]
    pub const fn independence(&self) -> (independence: &IndependenceSet)
        ensures independence.spec_values() == self.spec_independence(),
    { &self.independence }

    /// Returns the approval validity constraint.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    pub(crate) fn conjunction(
        &self,
        other: &Self,
    ) -> (result: Result<Option<Self>, PolicyError>)
        ensures
            result.is_ok(),
            match result {
                Ok(requirement) => crate::approval_model::approval_conjunction_result(
                    self,
                    other,
                    &requirement,
                ),
                Err(_) => true,
            },
    {
        let mut approver_roles = Vec::new();
        let mut left = 0;
        let mut right = 0;
        while left < self.approver_roles.len() && right < other.approver_roles.len()
            invariant
                0 <= left <= self.approver_roles.len(),
                0 <= right <= other.approver_roles.len(),
                approver_roles@ + crate::approval_model::role_intersection_from(
                    self.approver_roles@,
                    other.approver_roles@,
                    left as nat,
                    right as nat,
                ) == crate::approval_model::role_intersection_from(
                    self.approver_roles@,
                    other.approver_roles@,
                    0,
                    0,
                ),
            decreases
                (self.approver_roles.len() - left)
                    + (other.approver_roles.len() - right),
        {
            let left_rank = self.approver_roles[left].canonical_rank();
            let right_rank = other.approver_roles[right].canonical_rank();
            match left_rank.cmp(&right_rank) {
                Ordering::Less => left += 1,
                Ordering::Greater => right += 1,
                Ordering::Equal => {
                    approver_roles.push(self.approver_roles[left]);
                    left += 1;
                    right += 1;
                }
            }
        }
        if approver_roles.is_empty() {
            return Ok(None);
        }
        let validity = match self.validity.intersection(other.validity) {
            Ok(value) => value,
            Err(error) => {
                match error.kind() {
                    crate::PolicyErrorKind::InvalidValidityWindow
                    | crate::PolicyErrorKind::ClockEpochMismatch => return Ok(None),
                    _ => return Err(error),
                }
            }
        };
        let minimum_tier = self.minimum_tier.maximum(other.minimum_tier);
        let independence = self.independence.union(&other.independence);
        let requirement = Self {
            minimum_tier,
            approver_roles,
            independence,
            validity,
        };
        assert(crate::approval_model::approval_conjunction_result(
            self,
            other,
            &Some(requirement),
        ));
        Ok(Some(requirement))
    }

    pub(crate) fn constrain_validity(
        &self,
        validity_constraint: ValidityWindow,
    ) -> (result: Result<Option<Self>, PolicyError>)
        ensures
            result.is_ok(),
            match result {
                Ok(requirement) => crate::approval_model::constrained_approval_result(
                    self,
                    validity_constraint,
                    &requirement,
                ),
                Err(_) => true,
            },
    {
        let validity = match self.validity.intersection(validity_constraint) {
            Ok(value) => value,
            Err(error) => {
                match error.kind() {
                    crate::PolicyErrorKind::InvalidValidityWindow
                    | crate::PolicyErrorKind::ClockEpochMismatch => return Ok(None),
                    _ => return Err(error),
                }
            }
        };
        let mut approver_roles = Vec::new();
        let mut index = 0;
        while index < self.approver_roles.len()
            invariant
                0 <= index <= self.approver_roles.len(),
                approver_roles@ == self.approver_roles@.subrange(0, index as int),
            decreases self.approver_roles.len() - index,
        {
            approver_roles.push(self.approver_roles[index]);
            index += 1;
        }
        let independence = self.independence.duplicate();
        assert(approver_roles@ == self.spec_approver_roles());
        assert(independence.spec_values() == self.spec_independence());
        let requirement = Self {
            minimum_tier: self.minimum_tier,
            approver_roles,
            independence,
            validity,
        };
        assert(requirement.spec_validity().spec_not_before().spec_epoch()
            == crate::approval_model::intersection_not_before_epoch(
                self.spec_validity(),
                validity_constraint,
            ));
        assert(requirement.spec_validity().spec_expires_at().spec_epoch()
            == crate::approval_model::intersection_expires_epoch(
                self.spec_validity(),
                validity_constraint,
            ));
        assert(crate::approval_model::constrained_approval_result(
            self,
            validity_constraint,
            &Some(requirement),
        ));
        Ok(Some(requirement))
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures
            duplicate.spec_same_as(self),
            duplicate.spec_minimum_tier() == self.spec_minimum_tier(),
            duplicate.spec_approver_roles() == self.spec_approver_roles(),
            duplicate.spec_independence() == self.spec_independence(),
            duplicate.spec_validity() == self.spec_validity(),
    {
        let mut approver_roles = Vec::new();
        let mut index = 0;
        while index < self.approver_roles.len()
            invariant
                0 <= index <= self.approver_roles.len(),
                approver_roles@ == self.approver_roles@.subrange(0, index as int),
            decreases self.approver_roles.len() - index,
        {
            approver_roles.push(self.approver_roles[index]);
            index += 1;
        }
        assert(approver_roles@ == self.approver_roles@);
        let independence = self.independence.duplicate();
        let duplicate = Self {
            minimum_tier: self.minimum_tier,
            approver_roles,
            independence,
            validity: self.validity,
        };
        reveal(ApprovalRequirement::spec_same_as);
        duplicate
    }
}

} // verus!
