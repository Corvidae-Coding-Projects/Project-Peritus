//! Canonical exact permission pairs and logical use bounds.

use crate::{identity::compare_identifier_bytes, CanonicalCollection, PolicyError};
use core::cmp::Ordering;
use peritus_types::{CapabilityName, ResourceId};
use vstd::prelude::*;

verus! {

/// One exact resource and capability-name pair.
#[derive(Debug, Eq, PartialEq)]
pub struct Permission {
    resource_id: ResourceId,
    capability_name: CapabilityName,
}

impl Permission {
    /// Returns the verified canonical order used by executable collection checks.
    pub open spec fn spec_canonical_cmp(&self, other: &Self) -> Ordering {
        let resource_order = peritus_types::canonical_byte_order_from(
            self.spec_resource_id()@,
            other.spec_resource_id()@,
            0,
        );
        if resource_order == Ordering::Equal {
            peritus_types::canonical_byte_order_from(
                self.spec_capability_name(),
                other.spec_capability_name(),
                0,
            )
        } else {
            resource_order
        }
    }

    /// Returns the exact resource identifier bytes used by specifications.
    pub closed spec fn spec_resource_id(&self) -> [u8; 16] {
        self.resource_id.spec_bytes()
    }

    /// Returns the exact capability-name bytes used by specifications.
    pub closed spec fn spec_capability_name(&self) -> Seq<u8> {
        self.capability_name.spec_bytes()
    }

    /// Returns the validated character sequence used to prove exact duplication.
    pub closed spec fn spec_capability_value(&self) -> Seq<char> {
        self.capability_name.spec_value()
    }

    /// Creates an exact permission pair.
    #[must_use]
    pub const fn new(
        resource_id: ResourceId,
        capability_name: CapabilityName,
    ) -> (permission: Self)
        ensures
            permission.spec_resource_id() == resource_id.spec_bytes(),
            permission.spec_capability_name() == capability_name.spec_bytes(),
            permission.spec_capability_value() == capability_name.spec_value(),
    {
        Self { resource_id, capability_name }
    }

    /// Returns the exact resource identity.
    #[must_use]
    pub const fn resource_id(&self) -> (resource_id: ResourceId)
        ensures resource_id.spec_bytes() == self.spec_resource_id(),
    { self.resource_id }

    /// Returns the exact capability name.
    #[must_use]
    pub const fn capability_name(&self) -> (name: &CapabilityName)
        ensures name.spec_bytes() == self.spec_capability_name(),
    { &self.capability_name }

    /// Compares exact pairs in their verified canonical order.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> (result: Ordering)
        ensures result == self.spec_canonical_cmp(other),
    {
        match compare_identifier_bytes(self.resource_id.as_bytes(), other.resource_id.as_bytes()) {
            Ordering::Equal => self.capability_name.canonical_cmp(&other.capability_name),
            ordering => ordering,
        }
    }

    pub(crate) fn duplicate(&self) -> (permission: Self)
        ensures
            permission.spec_resource_id() == self.spec_resource_id(),
            permission.spec_capability_value() == self.spec_capability_value(),
            permission.spec_capability_name() == self.spec_capability_name(),
    {
        Self {
            resource_id: self.resource_id,
            capability_name: self.capability_name.clone(),
        }
    }
}

/// Returns the first duplicate or descending adjacent exact permission pair.
pub open spec fn first_permission_order_error(
    values: Seq<Permission>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else {
        match values[index as int - 1].spec_canonical_cmp(&values[index as int]) {
            Ordering::Less => first_permission_order_error(values, index + 1),
            Ordering::Equal => Some(crate::PolicyErrorKind::DuplicateCanonicalValue),
            Ordering::Greater => Some(crate::PolicyErrorKind::NonCanonicalOrder),
        }
    }
}

/// Returns the exact first validation failure for a checked permission set.
pub open spec fn permission_set_validation_error(
    values: Seq<Permission>,
) -> Option<crate::PolicyErrorKind> {
    if values.len() == 0 {
        Some(crate::PolicyErrorKind::EmptyCanonicalCollection)
    } else {
        first_permission_order_error(values, 1)
    }
}

pub(crate) proof fn canonical_equal_implies_sequence_equal_from(
    left: Seq<u8>,
    right: Seq<u8>,
    index: nat,
)
    requires
        index <= left.len(),
        index <= right.len(),
        left.subrange(0, index as int) == right.subrange(0, index as int),
        peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Equal,
    ensures left == right,
    decreases left.len() - index,
{
    if index >= left.len() {
        assert(index == left.len());
        assert(index == right.len());
        assert(left == left.subrange(0, left.len() as int));
        assert(right == right.subrange(0, right.len() as int));
    } else {
        assert(index < right.len());
        assert(left[index as int] == right[index as int]);
        assert(left.subrange(0, index as int + 1)
            == right.subrange(0, index as int + 1));
        assert(peritus_types::canonical_byte_order_from(left, right, index + 1)
            == Ordering::Equal);
        canonical_equal_implies_sequence_equal_from(left, right, index + 1);
    }
}

/// Nonempty, strictly sorted, duplicate-free exact permission pairs.
#[derive(Debug, Eq, PartialEq)]
pub struct PermissionSet {
    values: Vec<Permission>,
}

impl PermissionSet {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.values@.len() > 0 }

    /// Returns the exact ordered permission sequence used by specifications.
    pub closed spec fn spec_values(&self) -> Seq<Permission> { self.values@ }

    pub(crate) open spec fn spec_same_as(&self, other: &Self) -> bool {
        self.spec_values().len() == other.spec_values().len()
            && forall |index: int| 0 <= index < self.spec_values().len() ==> {
                &&& #[trigger] self.spec_values()[index].spec_resource_id()
                    == other.spec_values()[index].spec_resource_id()
                &&& self.spec_values()[index].spec_capability_value()
                    == other.spec_values()[index].spec_capability_value()
                &&& self.spec_values()[index].spec_capability_name()
                    == other.spec_values()[index].spec_capability_name()
            }
    }

    /// Returns whether the canonical sequence contains one exact comparator-equal pair.
    pub open spec fn spec_contains(&self, permission: &Permission) -> bool {
        exists |index: int|
            0 <= index < self.spec_values().len()
                && #[trigger] self.spec_values()[index].spec_canonical_cmp(permission)
                    == Ordering::Equal
    }

    pub(crate) proof fn contained_value_is_one_exact_pair(&self, permission: &Permission)
        requires self.spec_contains(permission),
        ensures
            exists |index: int| 0 <= index < self.spec_values().len()
                && #[trigger] self.spec_values()[index].spec_resource_id()
                    == permission.spec_resource_id()
                && self.spec_values()[index].spec_capability_name()
                    == permission.spec_capability_name(),
    {
        let index = choose |index: int| 0 <= index < self.spec_values().len()
            && #[trigger] self.spec_values()[index].spec_canonical_cmp(permission)
                == Ordering::Equal;
        let candidate = &self.spec_values()[index];
        let resource_order = peritus_types::canonical_byte_order_from(
            candidate.spec_resource_id()@,
            permission.spec_resource_id()@,
            0,
        );
        assert(resource_order == Ordering::Equal);
        assert(candidate.spec_resource_id()@.subrange(0, 0)
            == permission.spec_resource_id()@.subrange(0, 0));
        canonical_equal_implies_sequence_equal_from(
            candidate.spec_resource_id()@,
            permission.spec_resource_id()@,
            0,
        );
        assert(peritus_types::canonical_byte_order_from(
            candidate.spec_capability_name(),
            permission.spec_capability_name(),
            0,
        ) == Ordering::Equal);
        assert(candidate.spec_capability_name().subrange(0, 0)
            == permission.spec_capability_name().subrange(0, 0));
        canonical_equal_implies_sequence_equal_from(
            candidate.spec_capability_name(),
            permission.spec_capability_name(),
            0,
        );
        assert(candidate.spec_resource_id() == permission.spec_resource_id());
    }

    /// Returns exact whole-set containment used by authority-boundary specifications.
    pub open spec fn spec_is_subset_of(&self, other: &Self) -> bool {
        forall |index: int| 0 <= index < self.spec_values().len() ==>
            #[trigger] other.spec_contains(&self.spec_values()[index])
    }

    /// Validates a canonical nonempty permission set.
    ///
    /// # Errors
    ///
    /// Returns a precise empty, duplicate, or ordering failure.
    pub fn new(values: Vec<Permission>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(permissions) => {
                    permission_set_validation_error(values@).is_none()
                        && permissions.spec_values() == values@
                }
                Err(error) => {
                    permission_set_validation_error(values@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Permissions)
                        && error.spec_dimension().is_none()
                }
            },
    {
        if values.is_empty() {
            return Err(PolicyError::empty_canonical_collection(
                CanonicalCollection::Permissions,
            ));
        }
        let mut index = 1;
        while index < values.len()
            invariant
                1 <= index <= values.len(),
                values@.len() > 0,
                first_permission_order_error(values@, 1)
                    == first_permission_order_error(values@, index as nat),
            decreases values.len() - index,
        {
            match values[index - 1].canonical_cmp(&values[index]) {
                Ordering::Less => {},
                Ordering::Equal => {
                    return Err(PolicyError::duplicate_canonical_value(
                        CanonicalCollection::Permissions,
                    ));
                }
                Ordering::Greater => {
                    return Err(PolicyError::non_canonical_order(
                        CanonicalCollection::Permissions,
                    ));
                }
            }
            index += 1;
        }
        let permissions = Self { values };
        reveal(PermissionSet::spec_values);
        Ok(permissions)
    }

    /// Borrows exact pairs in canonical order.
    #[must_use]
    pub const fn as_slice(&self) -> (values: &[Permission])
        ensures
            values@ == self.spec_values(),
            values@.len() > 0,
    {
        proof { use_type_invariant(self); }
        self.values.as_slice()
    }

    /// Returns whether the set contains one exact pair.
    #[must_use]
    pub fn contains(&self, permission: &Permission) -> (result: bool)
        ensures result == self.spec_contains(permission),
    {
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] self.values@[prior].spec_canonical_cmp(permission)
                        != Ordering::Equal,
            decreases self.values.len() - index,
        {
            match self.values[index].canonical_cmp(permission) {
                Ordering::Less | Ordering::Greater => index += 1,
                Ordering::Equal => return true,
            }
        }
        false
    }

    /// Returns whether every exact pair is also present in `other`.
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> (result: bool)
        ensures result == self.spec_is_subset_of(other),
    {
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] other.spec_contains(&self.spec_values()[prior]),
            decreases self.values.len() - index,
        {
            if !other.contains(&self.values[index]) {
                assert(!other.spec_contains(&self.values[index as int]));
                assert(self.spec_values()[index as int] == self.values[index as int]);
                assert(!self.spec_is_subset_of(other)) by {
                    assert(exists |found: int| found == index
                        && 0 <= found < self.spec_values().len()
                        && !#[trigger] other.spec_contains(&self.spec_values()[found]));
                }
                return false;
            }
            index += 1;
        }
        true
    }

    pub(crate) fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_same_as(self),
    {
        proof { use_type_invariant(self); }
        let mut values: Vec<Permission> = Vec::new();
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                values@.len() == index,
                forall |prior: int| 0 <= prior < index ==> {
                    &&& #[trigger] values@[prior].spec_resource_id()
                        == self.values@[prior].spec_resource_id()
                    &&& values@[prior].spec_capability_value()
                        == self.values@[prior].spec_capability_value()
                    &&& values@[prior].spec_capability_name()
                        == self.values@[prior].spec_capability_name()
                },
            decreases self.values.len() - index,
        {
            values.push(self.values[index].duplicate());
            index += 1;
        }
        Self { values }
    }
}

} // verus!
