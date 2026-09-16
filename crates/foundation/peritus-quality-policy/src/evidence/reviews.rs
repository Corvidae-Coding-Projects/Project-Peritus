//! Review collection ordering and cross-review finding uniqueness.

use crate::{CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, ReviewObservation};
#[cfg(verus_only)]
use crate::canonical::collections;
use crate::canonical::order;
use core::cmp::Ordering;
use peritus_types::FindingId;
use vstd::prelude::*;

verus! {

fn finding_seen_before(values: &[ReviewObservation], end: usize, target: FindingId) -> (found: bool)
    requires end <= values.len(),
    ensures found == collections::finding_seen_before(values@, end as int, target.spec_bytes()@),
{
    let mut review = 0;
    while review < end
        invariant
            0 <= review <= end <= values.len(),
            forall |prior: int, finding: int| 0 <= prior < review
                && 0 <= finding < values@[prior].spec_findings().len() ==>
                #[trigger] values@[prior].spec_findings()[finding].spec_finding_id().spec_bytes()@ != target.spec_bytes()@,
        decreases end - review,
    {
        let findings = values[review].findings();
        let mut finding = 0;
        while finding < findings.len()
            invariant
                0 <= finding <= findings.len(),
                review < end <= values.len(),
                findings@ == values@[review as int].spec_findings(),
                forall |prior: int| 0 <= prior < finding ==>
                    #[trigger] findings@[prior].spec_finding_id().spec_bytes()@ != target.spec_bytes()@,
            decreases findings.len() - finding,
        {
            let id = findings[finding].finding_id().into_bytes();
            if matches!(order::compare(id.as_slice(), target.as_bytes().as_slice()), Ordering::Equal) {
                assert(values@[review as int].spec_findings()[finding as int].spec_finding_id().spec_bytes()@ == target.spec_bytes()@);
                return true;
            }
            finding += 1;
        }
        review += 1;
    }
    false
}

pub(super) fn validate(values: &[ReviewObservation]) -> (result: Result<(), EvidenceError>)
    ensures result.is_ok() == collections::reviews_canonical(values@),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 1 <= prior < index ==>
                order::byte_order(#[trigger] collections::review_keys(values@)[prior - 1], collections::review_keys(values@)[prior]) == Ordering::Less,
            forall |prior: int| 1 <= prior < index ==>
                #[trigger] values@[prior - 1].spec_cycle_ordinal() < values@[prior].spec_cycle_ordinal(),
            forall |prior: int| 0 <= prior < index ==> collections::review_findings_unique_at(values@, prior),
            forall |prior: int| 0 <= prior < index ==> #[trigger] values@[prior].spec_is_canonical(),
        decreases values.len() - index,
    {
        let _ = values[index].revision();
        if index > 0 {
            let previous = values[index - 1].cycle_id().into_bytes();
            let current = values[index].cycle_id().into_bytes();
            if let Err(error) = order::require_ascending(previous.as_slice(), current.as_slice(), CanonicalEvidenceCollection::Reviews, index) {
                assert(order::byte_order(collections::review_keys(values@)[index as int - 1], collections::review_keys(values@)[index as int]) != Ordering::Less);
                assert(!order::ordered(collections::review_keys(values@)));
                return Err(error);
            }
            let previous = values[index - 1].cycle_ordinal().get();
            let current = values[index].cycle_ordinal().get();
            if previous == current {
                assert(values@[index as int - 1].spec_cycle_ordinal() >= values@[index as int].spec_cycle_ordinal());
                return Err(EvidenceError::new(EvidenceErrorKind::DuplicateObservation, CanonicalEvidenceCollection::Reviews, index));
            }
            if previous > current {
                assert(values@[index as int - 1].spec_cycle_ordinal() >= values@[index as int].spec_cycle_ordinal());
                return Err(EvidenceError::new(EvidenceErrorKind::NonCanonicalOrder, CanonicalEvidenceCollection::Reviews, index));
            }
        }
        let findings = values[index].findings();
        let mut finding = 0;
        while finding < findings.len()
            invariant
                0 <= finding <= findings.len(), index < values.len(),
                findings@ == values@[index as int].spec_findings(),
                forall |prior: int| 0 <= prior < finding ==>
                    !collections::finding_seen_before(values@, index as int, #[trigger] findings@[prior].spec_finding_id().spec_bytes()@),
            decreases findings.len() - finding,
        {
            let id = findings[finding].finding_id();
            if finding_seen_before(values, index, id) {
                assert(collections::finding_seen_before(values@, index as int, values@[index as int].spec_findings()[finding as int].spec_finding_id().spec_bytes()@));
                assert(!collections::review_findings_unique_at(values@, index as int));
                return Err(EvidenceError::new(EvidenceErrorKind::DuplicateObservation, CanonicalEvidenceCollection::Findings, finding));
            }
            finding += 1;
        }
        index += 1;
    }
    Ok(())
}

} // verus!
