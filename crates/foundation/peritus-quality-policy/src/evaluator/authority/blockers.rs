//! Exact blocker resolution over current reviews and the contract severity threshold.

use super::waiver;

use crate::{AcceptanceEvidence, FindingDisposition, UnmetCondition};
use peritus_spec::{AcceptanceContract, FindingSeverity};
use peritus_types::RevisionTuple;

const fn rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Advisory => 0,
        FindingSeverity::Low => 1,
        FindingSeverity::Medium => 2,
        FindingSeverity::High => 3,
        FindingSeverity::Critical => 4,
    }
}
pub(super) fn evaluate(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;
    let threshold = rank(contract.review_policy().blocking_severity());
    let mut review_index = 0;
    while review_index < evidence.reviews().len() {
        let review = &evidence.reviews()[review_index];
        if crate::revision::revision_matches(review.revision(), requested) {
            let findings = review.findings();
            let mut index = 0;
            while index < findings.len() {
                let finding = &findings[index];
                if rank(finding.severity()) >= threshold {
                    match finding.disposition() {
                        FindingDisposition::Resolved { revision: _resolution, .. } => {}
                        FindingDisposition::Open | FindingDisposition::WaiverRequested => {
                            if !waiver::current_waiver_is_valid(
                                contract, requested, evidence, finding,
                            ) {
                                complete = false;
                                unmet.push(UnmetCondition::UnwaivedBlocker {
                                    finding_id: finding.finding_id(),
                                    severity: finding.severity(),
                                });
                            }
                        }
                    }
                }
                index += 1;
            }
        }
        review_index += 1;
    }
    complete
}
