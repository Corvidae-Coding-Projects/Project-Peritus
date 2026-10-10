//! Exact linear subset matching for canonical schema field identities.

use super::{SchemaEvidence, SchemaField, SchemaFieldId, SchemaRequirement};
use core::cmp::Ordering;

fn compare_at(
    required: &[SchemaField],
    required_index: usize,
    observed: &[SchemaFieldId],
    observed_index: usize,
) -> Ordering {
    crate::order::compare(
        observed[observed_index].digest().as_bytes(),
        required[required_index].id().digest().as_bytes(),
    )
}
pub(super) fn covers(evidence: &SchemaEvidence, requirement: &SchemaRequirement) -> bool {
    if !evidence.direction.same(requirement.direction()) {
        return false;
    }
    let required = requirement.fields();
    let observed = evidence.observed_fields();
    let mut required_index = 0;
    let mut observed_index = 0;
    while required_index < required.len() {
        if observed_index >= observed.len() {
            return false;
        }
        let order = compare_at(required, required_index, observed, observed_index);
        match order {
            Ordering::Equal => {
                required_index += 1;
                observed_index += 1;
            }
            Ordering::Less => {
                observed_index += 1;
            }
            Ordering::Greater => {
                return false;
            }
        }
    }
    true
}
