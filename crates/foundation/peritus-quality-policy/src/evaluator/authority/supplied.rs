//! Validation and diagnostic completeness for every supplied current waiver.

use super::waiver;

use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;

pub(super) fn evaluate(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < evidence.waivers().len() {
        let waiver = &evidence.waivers()[index];
        if crate::revision::revision_matches(waiver.revision(), requested) {
            let failure = waiver::supplied_failure(contract, requested, evidence, waiver);
            if let Some(reason) = failure {
                complete = false;
                unmet.push(UnmetCondition::InvalidWaiver {
                    finding_id: waiver.finding_id(),
                    reason,
                });
            }
        }
        index += 1;
    }
    complete
}
