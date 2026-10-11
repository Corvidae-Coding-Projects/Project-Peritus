//! Review collection ordering and cross-review finding uniqueness.

use crate::{CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, ReviewObservation};

use crate::canonical::order;
use core::cmp::Ordering;
use peritus_types::FindingId;

fn finding_seen_before(values: &[ReviewObservation], end: usize, target: FindingId) -> bool {
    let mut review = 0;
    while review < end {
        let findings = values[review].findings();
        let mut finding = 0;
        while finding < findings.len() {
            let id = findings[finding].finding_id().into_bytes();
            if matches!(
                order::compare(id.as_slice(), target.as_bytes().as_slice()),
                Ordering::Equal
            ) {
                return true;
            }
            finding += 1;
        }
        review += 1;
    }
    false
}
pub(super) fn validate(values: &[ReviewObservation]) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len() {
        let _ = values[index].revision();
        if index > 0 {
            let previous = values[index - 1].cycle_id().into_bytes();
            let current = values[index].cycle_id().into_bytes();
            if let Err(error) = order::require_ascending(
                previous.as_slice(),
                current.as_slice(),
                CanonicalEvidenceCollection::Reviews,
                index,
            ) {
                return Err(error);
            }
            let previous = values[index - 1].cycle_ordinal().get();
            let current = values[index].cycle_ordinal().get();
            if previous == current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
            if previous > current {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Reviews,
                    index,
                ));
            }
        }
        let findings = values[index].findings();
        let mut finding = 0;
        while finding < findings.len() {
            let id = findings[finding].finding_id();
            if finding_seen_before(values, index, id) {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Findings,
                    finding,
                ));
            }
            finding += 1;
        }
        index += 1;
    }
    Ok(())
}
