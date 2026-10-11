//! Public read-only operation-registry queries.

use crate::{ActorRole, OperationDescriptor, OperationRegistry, RiskClass, RiskSet};
use core::cmp::Ordering;
use peritus_types::CapabilityName;

fn descriptor_index_checked(
    descriptors: &[OperationDescriptor],
    name: &CapabilityName,
    index: usize,
) -> Option<usize> {
    if index == descriptors.len() {
        None
    } else {
        match descriptors[index].name_cmp(name) {
            Ordering::Less => {
                let result = descriptor_index_checked(descriptors, name, index + 1);
                result
            }
            Ordering::Equal => Some(index),
            Ordering::Greater => None,
        }
    }
}
impl RiskSet {
    #[doc = " Borrows the canonical risk values."]
    #[must_use]
    pub const fn as_slice(&self) -> &[RiskClass] {
        self.values.as_slice()
    }
}
impl OperationDescriptor {
    #[doc = " Returns the exact capability name."]
    #[must_use]
    pub const fn name(&self) -> &CapabilityName {
        &self.name
    }
    #[doc = " Returns the canonical risk classification."]
    #[must_use]
    pub const fn risks(&self) -> &RiskSet {
        &self.risks
    }
}
impl OperationRegistry {
    #[doc = " Returns the descriptor for an exact capability name."]
    #[must_use]
    pub fn descriptor_for(&self, name: &CapabilityName) -> Option<&OperationDescriptor> {
        let descriptors = self.as_slice();
        if let Some(index) = descriptor_index_checked(descriptors, name, 0) {
            return Some(&descriptors[index]);
        }
        None
    }
    #[doc = " Returns whether a role may receive the exact registered operation."]
    #[must_use]
    pub fn role_permits(&self, role: ActorRole, name: &CapabilityName) -> bool {
        let Some(descriptor) = self.descriptor_for(name) else {
            return false;
        };
        role.permits_operation(descriptor.operation_class())
    }
}
