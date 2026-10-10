//! Actual review constructor validations in their stable error precedence.

use crate::{
    CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, FindingDisposition,
    FindingObservation,
};

use crate::canonical::order;
use core::cmp::Ordering;
use peritus_spec::ReviewCategory;
use peritus_types::RevisionTuple;

pub(super) fn categories(values: &[ReviewCategory]) -> Result<(), EvidenceError> {
    if values.is_empty() {
        return Err(EvidenceError::new(
            EvidenceErrorKind::EmptyReviewCategories,
            CanonicalEvidenceCollection::ReviewCategories,
            0,
        ));
    }
    let mut index = 1;
    while index < values.len() {
        let previous = values[index - 1].digest().into_bytes();
        let current = values[index].digest().into_bytes();
        let comparison = order::compare(previous.as_slice(), current.as_slice());
        if !matches!(comparison, Ordering::Less) {}
        match comparison {
            Ordering::Equal => {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::ReviewCategories,
                    index,
                ));
            }
            Ordering::Greater => {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::ReviewCategories,
                    index,
                ));
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn findings(
    values: &[FindingObservation],
    revision: RevisionTuple,
) -> Result<(), EvidenceError> {
    let mut index = 0;
    while index < values.len() {
        match values[index].disposition() {
            FindingDisposition::Resolved { revision: resolution, .. }
                if !crate::revision::revision_matches(resolution, revision) =>
            {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::ResolutionRevisionMismatch,
                    CanonicalEvidenceCollection::Findings,
                    index,
                ));
            }
            _ => {}
        }
        if index > 0 {
            let previous = values[index - 1].finding_id().into_bytes();
            let current = values[index].finding_id().into_bytes();
            let comparison = order::compare(previous.as_slice(), current.as_slice());
            if !matches!(comparison, Ordering::Less) {}
            match comparison {
                Ordering::Equal => {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::DuplicateObservation,
                        CanonicalEvidenceCollection::Findings,
                        index,
                    ));
                }
                Ordering::Greater => {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::NonCanonicalOrder,
                        CanonicalEvidenceCollection::Findings,
                        index,
                    ));
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
