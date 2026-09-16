//! Exact checked admission for immutable worker descriptors.

use super::WorkerDescriptor;
use crate::{ExecutionClass, ResourceVector, WorkerId};
use peritus_types::ActorId;
use vstd::prelude::*;

verus! {

/// Internal classification before the public scheduler-error projection.
pub(super) enum DescriptorAdmission {
    Accepted(WorkerDescriptor),
    ClassesOrConcurrencyRejected,
    CapacityLimitExceeded,
}

/// Nonempty execution classes in strict canonical rank order.
pub open spec fn classes_canonical(classes: Seq<ExecutionClass>) -> bool {
    &&& classes.len() > 0
    &&& forall |index: int| #![trigger classes[index]] 1 <= index < classes.len() ==>
        classes[index - 1].spec_precedes(&classes[index])
}

fn classes_are_canonical(classes: &[ExecutionClass]) -> (result: bool)
    ensures result == classes_canonical(classes@),
{
    if classes.is_empty() {
        return false;
    }
    let mut index = 1usize;
    while index < classes.len()
        invariant
            1 <= index <= classes@.len(),
            forall |prior: int| #![trigger classes@[prior]] 1 <= prior < index ==>
                classes@[prior - 1].spec_precedes(&classes@[prior]),
        decreases classes@.len() - index,
    {
        if !classes[index - 1].precedes(classes[index]) {
            return false;
        }
        index += 1;
    }
    true
}

/// Performs the production worker checks before constructing the public error.
pub(super) fn admit_descriptor(
    id: WorkerId,
    owner: ActorId,
    classes: Vec<ExecutionClass>,
    capacity: ResourceVector,
    concurrency: u16,
    maximum_concurrency: u16,
    maximum_dimensions: u16,
) -> (result: DescriptorAdmission)
    ensures match result {
        DescriptorAdmission::Accepted(descriptor) => {
            &&& classes_canonical(classes@)
            &&& 0 < concurrency <= maximum_concurrency
            &&& crate::resource::capacity::entries_admissible(
                capacity.spec_entries(), maximum_dimensions,
            )
            &&& descriptor.spec_id() == id
            &&& descriptor.spec_owner() == owner
            &&& descriptor.spec_classes() == classes@
            &&& descriptor.spec_capacity().spec_entries() == capacity.spec_entries()
            &&& descriptor.spec_concurrency() == concurrency
        }
        DescriptorAdmission::ClassesOrConcurrencyRejected => {
            !classes_canonical(classes@)
                || concurrency == 0
                || concurrency > maximum_concurrency
        }
        DescriptorAdmission::CapacityLimitExceeded => {
            &&& classes_canonical(classes@)
            &&& 0 < concurrency <= maximum_concurrency
            &&& !crate::resource::capacity::entries_admissible(
                capacity.spec_entries(), maximum_dimensions,
            )
            &&& capacity.spec_entries().len() > maximum_dimensions as int
        }
    },
{
    if !classes_are_canonical(&classes)
        || concurrency == 0
        || concurrency > maximum_concurrency
    {
        return DescriptorAdmission::ClassesOrConcurrencyRejected;
    }
    if capacity.entries().len() > usize::from(maximum_dimensions) {
        return DescriptorAdmission::CapacityLimitExceeded;
    }
    DescriptorAdmission::Accepted(WorkerDescriptor {
        id,
        owner,
        classes,
        capacity,
        concurrency,
    })
}

} // verus!
