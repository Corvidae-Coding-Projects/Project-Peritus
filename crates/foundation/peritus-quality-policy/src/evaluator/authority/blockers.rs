//! Exact blocker resolution over current reviews and the contract severity threshold.

#[cfg(verus_only)]
use super::diagnostics;
use super::waiver;
#[cfg(verus_only)]
use crate::model::authority::*;
use crate::{AcceptanceEvidence, FindingDisposition, UnmetCondition};
use peritus_spec::{AcceptanceContract, FindingSeverity};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

const fn rank(severity: FindingSeverity) -> (value: u8)
    ensures value as nat == severity_rank(severity),
{
    match severity {
        FindingSeverity::Advisory => 0, FindingSeverity::Low => 1,
        FindingSeverity::Medium => 2, FindingSeverity::High => 3, FindingSeverity::Critical => 4,
    }
}

proof fn freshness_transitive(left: RevisionTuple, middle: RevisionTuple, right: RevisionTuple)
    requires crate::model::revision_fresh(left, middle), crate::model::revision_fresh(middle, right),
    ensures crate::model::revision_fresh(left, right),
{
    reveal_with_fuel(crate::revision::same_identifier_from, 17);
}

pub(super) fn evaluate(contract: &AcceptanceContract, requested: RevisionTuple, evidence: &AcceptanceEvidence, unmet: &mut Vec<UnmetCondition>) -> (complete: bool)
    ensures complete == blocking_findings_resolved(contract, requested, evidence),
        diagnostics::preserved(old(unmet)@, final(unmet)@),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let threshold = rank(contract.review_policy().blocking_severity());
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant
            review_index <= evidence.spec_reviews().len(),
            threshold as nat == severity_rank(contract.spec_review_policy().spec_blocking_severity()),
            diagnostics::preserved(old(unmet)@, unmet@),
            complete == (forall |review: int, index: int| 0 <= review < review_index
                && 0 <= index < evidence.spec_reviews()[review].spec_findings().len()
                && crate::model::revision_fresh(evidence.spec_reviews()[review].spec_revision(), requested)
                && severity_rank(#[trigger] evidence.spec_reviews()[review].spec_findings()[index].spec_severity()) >= threshold
                ==> finding_resolved_or_waived(contract, requested, evidence, evidence.spec_reviews()[review].spec_findings()[index])),
            complete ==> unmet@ == old(unmet)@,
        decreases evidence.spec_reviews().len() - review_index,
    {
        let review = &evidence.reviews()[review_index];
        if crate::revision::revision_matches(review.revision(), requested) {
            proof { review.canonical_views(); }
            let findings = review.findings();
            let mut index = 0;
            while index < findings.len()
                invariant
                    index <= findings.len(), review_index < evidence.spec_reviews().len(),
                    *review == evidence.spec_reviews()[review_index as int],
                    findings@ == review.spec_findings(),
                    crate::canonical::reviews::review_admissible(review.spec_categories(), findings@, review.spec_revision()),
                    crate::model::revision_fresh(review.spec_revision(), requested),
                    threshold as nat == severity_rank(contract.spec_review_policy().spec_blocking_severity()),
                    diagnostics::preserved(old(unmet)@, unmet@),
                    complete == ((forall |prior_review: int, prior_index: int| 0 <= prior_review < review_index
                        && 0 <= prior_index < evidence.spec_reviews()[prior_review].spec_findings().len()
                        && crate::model::revision_fresh(evidence.spec_reviews()[prior_review].spec_revision(), requested)
                        && severity_rank(#[trigger] evidence.spec_reviews()[prior_review].spec_findings()[prior_index].spec_severity()) >= threshold
                        ==> finding_resolved_or_waived(contract, requested, evidence, evidence.spec_reviews()[prior_review].spec_findings()[prior_index]))
                        && (forall |prior: int| 0 <= prior < index
                            && severity_rank(#[trigger] findings@[prior].spec_severity()) >= threshold
                            ==> finding_resolved_or_waived(contract, requested, evidence, findings@[prior]))),
                    complete ==> unmet@ == old(unmet)@,
                decreases findings.len() - index,
            {
                let finding = &findings[index];
                if rank(finding.severity()) >= threshold {
                    match finding.disposition() {
                        FindingDisposition::Resolved { revision: _resolution, .. } => {
                            proof { freshness_transitive(_resolution, review.spec_revision(), requested); };
                        },
                        FindingDisposition::Open | FindingDisposition::WaiverRequested => {
                            if !waiver::current_waiver_is_valid(contract, requested, evidence, finding) {
                                complete = false;
                                unmet.push(UnmetCondition::UnwaivedBlocker { finding_id: finding.finding_id(), severity: finding.severity() });
                            }
                        },
                    }
                }
                index += 1;
            }
        }
        review_index += 1;
    }
    complete
}

} // verus!
