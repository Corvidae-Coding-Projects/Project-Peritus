//! Public read-only operation-registry queries.

use crate::{ActorRole, OperationDescriptor, OperationRegistry, RiskClass, RiskSet};
use core::cmp::Ordering;
use peritus_types::CapabilityName;
use vstd::prelude::*;

verus! {

pub open spec fn descriptor_for_from(
    descriptors: Seq<OperationDescriptor>,
    name: Seq<u8>,
    index: nat,
) -> Option<OperationDescriptor>
    decreases descriptors.len() - index,
{
    if index >= descriptors.len() {
        None
    } else {
        match descriptors[index as int].spec_name_cmp(name) {
            Ordering::Less => descriptor_for_from(descriptors, name, index + 1),
            Ordering::Equal => Some(descriptors[index as int]),
            Ordering::Greater => None,
        }
    }
}

fn descriptor_index_checked(
    descriptors: &[OperationDescriptor],
    name: &CapabilityName,
    index: usize,
) -> (result: Option<usize>)
    requires index <= descriptors.len(),
    ensures
        match result {
            Some(found) => {
                index <= found < descriptors.len()
                    && descriptor_for_from(descriptors@, name.spec_bytes(), index as nat)
                        == Some(descriptors@[found as int])
            }
            None => descriptor_for_from(
                descriptors@,
                name.spec_bytes(),
                index as nat,
            ).is_none(),
        },
    decreases descriptors.len() - index,
{
    if index == descriptors.len() {
        None
    } else {
        match descriptors[index].name_cmp(name) {
            Ordering::Less => {
                let result = descriptor_index_checked(descriptors, name, index + 1);
                proof {
                    assert(descriptor_for_from(
                        descriptors@,
                        name.spec_bytes(),
                        index as nat,
                    ) == descriptor_for_from(
                        descriptors@,
                        name.spec_bytes(),
                        (index + 1) as nat,
                    ));
                    match result {
                        Some(found) => assert(index <= found),
                        None => {},
                    }
                }
                result
            }
            Ordering::Equal => Some(index),
            Ordering::Greater => None,
        }
    }
}

impl RiskSet {
    /// Borrows the canonical risk values.
    #[must_use]
    pub const fn as_slice(&self) -> &[RiskClass] { self.values.as_slice() }
}

impl OperationDescriptor {
    /// Returns the exact capability name.
    #[must_use]
    pub const fn name(&self) -> &CapabilityName { &self.name }

    /// Returns the canonical risk classification.
    #[must_use]
    pub const fn risks(&self) -> &RiskSet { &self.risks }
}

impl OperationRegistry {
    /// Returns the exact descriptor lookup result used by public queries.
    pub open spec fn spec_descriptor_for(
        &self,
        name: &CapabilityName,
    ) -> Option<OperationDescriptor> {
        descriptor_for_from(self.spec_descriptors(), name.spec_bytes(), 0)
    }

    /// Returns the exact compiled role-separation answer for one registry name.
    pub open spec fn spec_role_permits(
        &self,
        role: ActorRole,
        name: &CapabilityName,
    ) -> bool {
        match self.spec_descriptor_for(name) {
            Some(descriptor) => crate::model::role_permits(
                role,
                descriptor.spec_operation_class(),
            ),
            None => false,
        }
    }

    /// Returns the descriptor for an exact capability name.
    #[must_use]
    pub fn descriptor_for(
        &self,
        name: &CapabilityName,
    ) -> (result: Option<&OperationDescriptor>)
        ensures
            match result {
                Some(descriptor) => self.spec_descriptor_for(name) == Some(*descriptor),
                None => self.spec_descriptor_for(name).is_none(),
            },
    {
        let descriptors = self.as_slice();
        if let Some(index) = descriptor_index_checked(descriptors, name, 0) {
            return Some(&descriptors[index]);
        }
        None
    }

    /// Returns whether a role may receive the exact registered operation.
    #[must_use]
    pub fn role_permits(&self, role: ActorRole, name: &CapabilityName) -> (result: bool)
        ensures result == self.spec_role_permits(role, name),
    {
        let Some(descriptor) = self.descriptor_for(name) else {
            return false;
        };
        role.permits_operation(descriptor.operation_class())
    }
}

} // verus!
