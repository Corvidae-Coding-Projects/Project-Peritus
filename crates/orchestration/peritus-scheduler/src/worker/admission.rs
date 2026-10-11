//! Exact checked admission for immutable worker descriptors.

use super::WorkerDescriptor;
use crate::{ExecutionClass, ResourceVector, WorkerId};
use peritus_types::ActorId;

#[doc = " Internal classification before the public scheduler-error projection."]
pub(super) enum DescriptorAdmission {
    Accepted(WorkerDescriptor),
    ClassesOrConcurrencyRejected,
    CapacityLimitExceeded,
}
fn classes_are_canonical(classes: &[ExecutionClass]) -> bool {
    if classes.is_empty() {
        return false;
    }
    let mut index = 1usize;
    while index < classes.len() {
        if !classes[index - 1].precedes(classes[index]) {
            return false;
        }
        index += 1;
    }
    true
}
#[doc = " Performs the production worker checks before constructing the public error."]
pub(super) fn admit_descriptor(
    id: WorkerId,
    owner: ActorId,
    classes: Vec<ExecutionClass>,
    capacity: ResourceVector,
    concurrency: u16,
    maximum_concurrency: u16,
    maximum_dimensions: u16,
) -> DescriptorAdmission {
    if !classes_are_canonical(&classes) || concurrency == 0 || concurrency > maximum_concurrency {
        return DescriptorAdmission::ClassesOrConcurrencyRejected;
    }
    if capacity.entries().len() > usize::from(maximum_dimensions) {
        return DescriptorAdmission::CapacityLimitExceeded;
    }
    DescriptorAdmission::Accepted(WorkerDescriptor { id, owner, classes, capacity, concurrency })
}
