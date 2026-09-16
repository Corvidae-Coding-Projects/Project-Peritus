//! Actual review constructor validations in their stable error precedence.

use crate::{CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, FindingDisposition, FindingObservation};
#[cfg(verus_only)]
use crate::canonical::reviews;
use crate::canonical::order;
use core::cmp::Ordering;
use peritus_spec::ReviewCategory;
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub(super) fn categories(values: &[ReviewCategory]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == reviews::categories_canonical(values@),
{
    if values.is_empty() {
        return Err(EvidenceError::new(EvidenceErrorKind::EmptyReviewCategories,
            CanonicalEvidenceCollection::ReviewCategories, 0));
    }
    let mut index = 1;
    while index < values.len()
        invariant
            1 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index ==>
                order::byte_order(#[trigger] reviews::category_keys(values@)[prior - 1],
                    reviews::category_keys(values@)[prior]) == Ordering::Less,
        decreases values.len() - index,
    {
        let previous = values[index - 1].digest().into_bytes();
        let current = values[index].digest().into_bytes();
        let comparison = order::compare(previous.as_slice(), current.as_slice());
        assert(comparison == order::byte_order(
            reviews::category_keys(values@)[index as int - 1], reviews::category_keys(values@)[index as int]));
        if !matches!(comparison, Ordering::Less) {
            assert(!order::ordered(reviews::category_keys(values@))) by {
                assert(order::byte_order(reviews::category_keys(values@)[index as int - 1],
                    reviews::category_keys(values@)[index as int]) != Ordering::Less);
            };
        }
        match comparison {
            Ordering::Equal => return Err(EvidenceError::new(EvidenceErrorKind::DuplicateObservation,
                CanonicalEvidenceCollection::ReviewCategories, index)),
            Ordering::Greater => return Err(EvidenceError::new(EvidenceErrorKind::NonCanonicalOrder,
                CanonicalEvidenceCollection::ReviewCategories, index)),
            Ordering::Less => {},
        }
        index += 1;
    }
    Ok(())
}

pub(super) fn findings(values: &[FindingObservation], revision: RevisionTuple) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == reviews::findings_canonical(values@, revision),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                reviews::resolution_current(#[trigger] values@[prior], revision),
            forall |prior: int| 1 <= prior < index ==>
                order::byte_order(#[trigger] reviews::finding_keys(values@)[prior - 1],
                    reviews::finding_keys(values@)[prior]) == Ordering::Less,
        decreases values.len() - index,
    {
        match values[index].disposition() {
            FindingDisposition::Resolved { revision: resolution, .. }
                if !crate::revision::revision_matches(resolution, revision) => {
                return Err(EvidenceError::new(EvidenceErrorKind::ResolutionRevisionMismatch,
                    CanonicalEvidenceCollection::Findings, index));
            },
            _ => {},
        }
        if index > 0 {
            let previous = values[index - 1].finding_id().into_bytes();
            let current = values[index].finding_id().into_bytes();
            let comparison = order::compare(previous.as_slice(), current.as_slice());
            assert(comparison == order::byte_order(
                reviews::finding_keys(values@)[index as int - 1], reviews::finding_keys(values@)[index as int]));
            if !matches!(comparison, Ordering::Less) {
                assert(!order::ordered(reviews::finding_keys(values@))) by {
                    assert(order::byte_order(reviews::finding_keys(values@)[index as int - 1],
                        reviews::finding_keys(values@)[index as int]) != Ordering::Less);
                };
            }
            match comparison {
                Ordering::Equal => return Err(EvidenceError::new(EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::Findings, index)),
                Ordering::Greater => return Err(EvidenceError::new(EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::Findings, index)),
                Ordering::Less => {},
            }
        }
        index += 1;
    }
    Ok(())
}

} // verus!
