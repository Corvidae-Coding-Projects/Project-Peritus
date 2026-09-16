//! Exact gate and artifact collection ordering, preserving first-error precedence.

use crate::{CanonicalEvidenceCollection, EvidenceError, EvidenceObservation, GateObservation};
#[cfg(verus_only)]
use crate::canonical::collections;
use crate::canonical::order;
#[cfg(verus_only)]
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(super) fn validate_gates(values: &[GateObservation]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == order::ordered(collections::gate_keys(values@)),
{
    let mut index = 1;
    while index < values.len()
        invariant
            (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index && prior < values.len() ==>
                order::byte_order(#[trigger] collections::gate_keys(values@)[prior - 1], collections::gate_keys(values@)[prior]) == Ordering::Less,
        decreases values.len() - index,
    {
        let previous = values[index - 1].gate_id().into_bytes();
        let current = values[index].gate_id().into_bytes();
        if let Err(error) = order::require_ascending(previous.as_slice(), current.as_slice(), CanonicalEvidenceCollection::Gates, index) {
            assert(order::byte_order(collections::gate_keys(values@)[index as int - 1], collections::gate_keys(values@)[index as int]) != Ordering::Less);
            assert(!order::ordered(collections::gate_keys(values@)));
            return Err(error);
        }
        index += 1;
    }
    Ok(())
}

pub(super) fn validate_evidence(values: &[EvidenceObservation]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == order::ordered(collections::artifact_keys(values@)),
{
    let mut index = 1;
    while index < values.len()
        invariant
            (values.len() == 0 && index == 1) || 1 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index && prior < values.len() ==>
                order::byte_order(#[trigger] collections::artifact_keys(values@)[prior - 1], collections::artifact_keys(values@)[prior]) == Ordering::Less,
        decreases values.len() - index,
    {
        let previous = values[index - 1].requirement_id().digest().into_bytes();
        let current = values[index].requirement_id().digest().into_bytes();
        if let Err(error) = order::require_ascending(previous.as_slice(), current.as_slice(), CanonicalEvidenceCollection::Evidence, index) {
            assert(order::byte_order(collections::artifact_keys(values@)[index as int - 1], collections::artifact_keys(values@)[index as int]) != Ordering::Less);
            assert(!order::ordered(collections::artifact_keys(values@)));
            return Err(error);
        }
        index += 1;
    }
    Ok(())
}

} // verus!
