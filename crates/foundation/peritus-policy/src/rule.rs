//! Restriction-only policy rules and approval conjunction values.

use crate::{
    ActorRole, IndependenceSet, PolicyError, ValidityWindow, scope_validation::validate_role_values,
};

use core::cmp::Ordering;

#[doc = " Minimum authenticated authority tier required by an escalation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AuthorityTier {
    #[doc = " Authenticated project authority."]
    Project,
    #[doc = " Authenticated user authority."]
    User,
    #[doc = " Authenticated organization authority."]
    Organization,
    #[doc = " Protected system authority."]
    System,
}
impl AuthorityTier {
    #[doc = " Checks whether this authenticated tier satisfies an exact minimum authority tier."]
    #[must_use]
    pub const fn at_least(self, required: Self) -> bool {
        self.rank() >= required.rank()
    }
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::Project => 0,
            Self::User => 1,
            Self::Organization => 2,
            Self::System => 3,
        }
    }
    pub(crate) const fn maximum(self, other: Self) -> Self {
        if self.rank() >= other.rank() { self } else { other }
    }
}
#[doc = " One approval rule\'s tier, role, independence, and validity constraints."]
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalRequirement {
    minimum_tier: AuthorityTier,
    approver_roles: Vec<ActorRole>,
    independence: IndependenceSet,
    validity: ValidityWindow,
}
impl ApprovalRequirement {
    #[doc = " Creates a requirement with canonical nonempty approver roles."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise role-set validation failure."]
    pub fn new(
        minimum_tier: AuthorityTier,
        approver_roles: Vec<ActorRole>,
        independence: IndependenceSet,
        validity: ValidityWindow,
    ) -> Result<Self, PolicyError> {
        validate_role_values(approver_roles.as_slice())?;
        let requirement = Self { minimum_tier, approver_roles, independence, validity };
        Ok(requirement)
    }
    #[doc = " Returns the minimum authority tier."]
    #[must_use]
    pub const fn minimum_tier(&self) -> AuthorityTier {
        self.minimum_tier
    }
    #[doc = " Borrows canonical allowed approver roles."]
    #[must_use]
    pub const fn approver_roles(&self) -> &[ActorRole] {
        self.approver_roles.as_slice()
    }
    #[doc = " Returns the independence conjunction."]
    #[must_use]
    pub const fn independence(&self) -> &IndependenceSet {
        &self.independence
    }
    #[doc = " Returns the approval validity constraint."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    pub(crate) fn conjunction(&self, other: &Self) -> Result<Option<Self>, PolicyError> {
        let mut approver_roles = Vec::new();
        let mut left = 0;
        let mut right = 0;
        while left < self.approver_roles.len() && right < other.approver_roles.len() {
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
            Err(error) => match error.kind() {
                crate::PolicyErrorKind::InvalidValidityWindow
                | crate::PolicyErrorKind::ClockEpochMismatch => return Ok(None),
                _ => return Err(error),
            },
        };
        let minimum_tier = self.minimum_tier.maximum(other.minimum_tier);
        let independence = self.independence.union(&other.independence);
        let requirement = Self { minimum_tier, approver_roles, independence, validity };
        Ok(Some(requirement))
    }
    pub(crate) fn constrain_validity(
        &self,
        validity_constraint: ValidityWindow,
    ) -> Result<Option<Self>, PolicyError> {
        let validity = match self.validity.intersection(validity_constraint) {
            Ok(value) => value,
            Err(error) => match error.kind() {
                crate::PolicyErrorKind::InvalidValidityWindow
                | crate::PolicyErrorKind::ClockEpochMismatch => return Ok(None),
                _ => return Err(error),
            },
        };
        let mut approver_roles = Vec::new();
        let mut index = 0;
        while index < self.approver_roles.len() {
            approver_roles.push(self.approver_roles[index]);
            index += 1;
        }
        let independence = self.independence.duplicate();
        let requirement =
            Self { minimum_tier: self.minimum_tier, approver_roles, independence, validity };
        Ok(Some(requirement))
    }
    pub(crate) fn duplicate(&self) -> Self {
        let mut approver_roles = Vec::new();
        let mut index = 0;
        while index < self.approver_roles.len() {
            approver_roles.push(self.approver_roles[index]);
            index += 1;
        }
        let independence = self.independence.duplicate();

        Self {
            minimum_tier: self.minimum_tier,
            approver_roles,
            independence,
            validity: self.validity,
        }
    }
}
