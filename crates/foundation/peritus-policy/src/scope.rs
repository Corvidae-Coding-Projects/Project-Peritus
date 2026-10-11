//! Canonical exact permission pairs and logical use bounds.

use crate::{CanonicalCollection, PolicyError, identity::compare_identifier_bytes};
use core::cmp::Ordering;
use peritus_types::{CapabilityName, ResourceId};

#[doc = " One exact resource and capability-name pair."]
#[derive(Debug, Eq, PartialEq)]
pub struct Permission {
    resource_id: ResourceId,
    capability_name: CapabilityName,
}
impl Permission {
    #[doc = " Creates an exact permission pair."]
    #[must_use]
    pub const fn new(resource_id: ResourceId, capability_name: CapabilityName) -> Self {
        Self { resource_id, capability_name }
    }
    #[doc = " Returns the exact resource identity."]
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        self.resource_id
    }
    #[doc = " Returns the exact capability name."]
    #[must_use]
    pub const fn capability_name(&self) -> &CapabilityName {
        &self.capability_name
    }
    #[doc = " Compares exact pairs in their verified canonical order."]
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        match compare_identifier_bytes(self.resource_id.as_bytes(), other.resource_id.as_bytes()) {
            Ordering::Equal => self.capability_name.canonical_cmp(&other.capability_name),
            ordering => ordering,
        }
    }
    pub(crate) fn duplicate(&self) -> Self {
        Self { resource_id: self.resource_id, capability_name: self.capability_name.clone() }
    }
}
#[doc = " Nonempty, strictly sorted, duplicate-free exact permission pairs."]
#[derive(Debug, Eq, PartialEq)]
pub struct PermissionSet {
    values: Vec<Permission>,
}
impl PermissionSet {
    #[doc = " Validates a canonical nonempty permission set."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise empty, duplicate, or ordering failure."]
    pub fn new(values: Vec<Permission>) -> Result<Self, PolicyError> {
        if values.is_empty() {
            return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Permissions));
        }
        let mut index = 1;
        while index < values.len() {
            match values[index - 1].canonical_cmp(&values[index]) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(PolicyError::duplicate_canonical_value(
                        CanonicalCollection::Permissions,
                    ));
                }
                Ordering::Greater => {
                    return Err(PolicyError::non_canonical_order(CanonicalCollection::Permissions));
                }
            }
            index += 1;
        }
        let permissions = Self { values };
        Ok(permissions)
    }
    #[doc = " Borrows exact pairs in canonical order."]
    #[must_use]
    pub const fn as_slice(&self) -> &[Permission] {
        self.values.as_slice()
    }
    #[doc = " Returns whether the set contains one exact pair."]
    #[must_use]
    pub fn contains(&self, permission: &Permission) -> bool {
        let mut index = 0;
        while index < self.values.len() {
            match self.values[index].canonical_cmp(permission) {
                Ordering::Less | Ordering::Greater => index += 1,
                Ordering::Equal => return true,
            }
        }
        false
    }
    #[doc = " Returns whether every exact pair is also present in `other`."]
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        let mut index = 0;
        while index < self.values.len() {
            if !other.contains(&self.values[index]) {
                return false;
            }
            index += 1;
        }
        true
    }
    pub(crate) fn duplicate(&self) -> Self {
        let mut values: Vec<Permission> = Vec::new();
        let mut index = 0;
        while index < self.values.len() {
            values.push(self.values[index].duplicate());
            index += 1;
        }
        Self { values }
    }
}
