//! Verified equality for review, finding, and waiver observations.

use super::{FindingDisposition, FindingObservation, FindingSeverity, WaiverObservation};
use vstd::prelude::*;

verus! {

/// Executable equality of every supplied finding field.
pub const fn findings_equal(left: &FindingObservation, right: &FindingObservation) -> (equal: bool)
    ensures equal == left.spec_matches(*right),
{
    crate::identity::finding_ids_equal(left.id(), right.id())
        && crate::evidence::bindings_equal(&left.binding(), &right.binding())
        && crate::identity::principal_ids_equal(left.reporter(), right.reporter())
        && finding_severities_equal(left.severity(), right.severity())
        && left.release_blocking() == right.release_blocking()
        && finding_dispositions_equal(left.disposition(), right.disposition())
        && crate::candidate::equality::digests_equal(
            left.finding_digest(),
            right.finding_digest(),
        )
}

/// Executable equality of every supplied waiver field.
pub const fn waivers_equal(left: &WaiverObservation, right: &WaiverObservation) -> (equal: bool)
    ensures equal == left.spec_matches(*right),
{
    crate::identity::finding_ids_equal(left.finding_id(), right.finding_id())
        && crate::evidence::bindings_equal(&left.binding(), &right.binding())
        && crate::identity::principal_ids_equal(left.authority(), right.authority())
        && crate::candidate::equality::digests_equal(
            left.waiver_digest(),
            right.waiver_digest(),
        )
        && crate::candidate::equality::digests_equal(
            left.justification_digest(),
            right.justification_digest(),
        )
        && left.approved() == right.approved()
}

/// Executable equality for finding severity.
pub const fn finding_severities_equal(
    left: FindingSeverity,
    right: FindingSeverity,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (FindingSeverity::Informational, FindingSeverity::Informational)
            | (FindingSeverity::Low, FindingSeverity::Low)
            | (FindingSeverity::Medium, FindingSeverity::Medium)
            | (FindingSeverity::High, FindingSeverity::High)
            | (FindingSeverity::Critical, FindingSeverity::Critical))
}

/// Executable equality for finding disposition.
pub const fn finding_dispositions_equal(
    left: FindingDisposition,
    right: FindingDisposition,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (FindingDisposition::Open, FindingDisposition::Open)
            | (FindingDisposition::Resolved, FindingDisposition::Resolved)
            | (FindingDisposition::WaiverRequested, FindingDisposition::WaiverRequested)
            | (FindingDisposition::Ignored, FindingDisposition::Ignored)
            | (FindingDisposition::Quarantined, FindingDisposition::Quarantined))
}

} // verus!
