//! Canonical categories, findings, and exact resolution freshness.

#[cfg(verus_only)]
use crate::{FindingDisposition, FindingObservation};
#[cfg(verus_only)]
use peritus_spec::ReviewCategory;
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Complete content-addressed category identities in their supplied order.
pub open spec fn category_keys(categories: Seq<ReviewCategory>) -> Seq<Seq<u8>> {
    categories.map(|index: int, category: ReviewCategory| category.spec_digest().spec_bytes()@)
}

/// Complete finding identities in their supplied order.
pub open spec fn finding_keys(findings: Seq<FindingObservation>) -> Seq<Seq<u8>> {
    findings.map(|index: int, finding: FindingObservation| finding.spec_finding_id().spec_bytes()@)
}

/// Canonical categories are nonempty and strictly ordered by every identity byte.
pub open spec fn categories_canonical(categories: Seq<ReviewCategory>) -> bool {
    categories.len() > 0 && super::order::ordered(category_keys(categories))
}

/// Any resolution was checked against the entire enclosing review revision.
pub open spec fn resolution_current(finding: FindingObservation, revision: RevisionTuple) -> bool {
    match finding.spec_disposition() {
        FindingDisposition::Resolved { revision: resolution, .. } => crate::model::revision_fresh(resolution, revision),
        _ => true,
    }
}

/// Findings may be empty; otherwise identities ascend and each resolution is current.
pub open spec fn findings_canonical(findings: Seq<FindingObservation>, revision: RevisionTuple) -> bool {
    super::order::ordered(finding_keys(findings))
        && forall |index: int| 0 <= index < findings.len() ==>
            resolution_current(#[trigger] findings[index], revision)
}

/// Exact admission policy of a normalized review constructor.
pub open spec fn review_admissible(
    categories: Seq<ReviewCategory>, findings: Seq<FindingObservation>, revision: RevisionTuple,
) -> bool {
    categories_canonical(categories) && findings_canonical(findings, revision)
}

/// Canonical categories and findings have globally unique identities.
pub proof fn canonical_implies_unique(categories: Seq<ReviewCategory>, findings: Seq<FindingObservation>, revision: RevisionTuple)
    requires review_admissible(categories, findings, revision),
    ensures super::order::unique(category_keys(categories)), super::order::unique(finding_keys(findings)),
{
    super::order::ordered_implies_unique(category_keys(categories), 32);
    super::order::ordered_implies_unique(finding_keys(findings), 16);
}

} // verus!
