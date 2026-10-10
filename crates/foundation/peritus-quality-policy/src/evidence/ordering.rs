//! Exact gate and artifact collection ordering, preserving first-error precedence.

use crate::{CanonicalEvidenceCollection, EvidenceError, EvidenceObservation, GateObservation};

use crate::canonical::order;

pub(super) fn validate_gates(values: &[GateObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len() {
        let previous = values[index - 1].gate_id().into_bytes();
        let current = values[index].gate_id().into_bytes();
        if let Err(error) = order::require_ascending(
            previous.as_slice(),
            current.as_slice(),
            CanonicalEvidenceCollection::Gates,
            index,
        ) {
            return Err(error);
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn validate_evidence(values: &[EvidenceObservation]) -> Result<(), EvidenceError> {
    let mut index = 1;
    while index < values.len() {
        let previous = values[index - 1].requirement_id().digest().into_bytes();
        let current = values[index].requirement_id().digest().into_bytes();
        if let Err(error) = order::require_ascending(
            previous.as_slice(),
            current.as_slice(),
            CanonicalEvidenceCollection::Evidence,
            index,
        ) {
            return Err(error);
        }
        index += 1;
    }
    Ok(())
}
