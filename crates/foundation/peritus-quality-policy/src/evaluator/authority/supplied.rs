//! Validation and diagnostic completeness for every supplied current waiver.

#[cfg(verus_only)]
use super::diagnostics;
use super::waiver;
#[cfg(verus_only)]
use crate::model::authority::*;
use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub(super) fn evaluate(contract: &AcceptanceContract, requested: RevisionTuple, evidence: &AcceptanceEvidence, unmet: &mut Vec<UnmetCondition>) -> (complete: bool)
    ensures
        complete == supplied_current_waivers_valid(contract, requested, evidence),
        diagnostics::preserved(old(unmet)@, final(unmet)@),
        invalid_waivers_reported(contract, requested, evidence, final(unmet)@),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    // Validate all current waivers, including findings below the blocking threshold.
    let mut index = 0;
    while index < evidence.waivers().len()
        invariant
            index <= evidence.spec_waivers().len(),
            diagnostics::preserved(old(unmet)@, unmet@),
            complete == (forall |prior: int| 0 <= prior < index
                && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[prior].spec_revision(), requested)
                ==> supplied_waiver_valid(contract, requested, evidence, evidence.spec_waivers()[prior])),
            forall |prior: int| 0 <= prior < index
                && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[prior].spec_revision(), requested)
                && !supplied_waiver_valid(contract, requested, evidence, evidence.spec_waivers()[prior])
                ==> invalid_waiver_reported(unmet@, evidence.spec_waivers()[prior].spec_finding_id()),
            complete ==> unmet@ == old(unmet)@,
        decreases evidence.spec_waivers().len() - index,
    {
        let ghost before = unmet@;
        let waiver = &evidence.waivers()[index];
        if crate::revision::revision_matches(waiver.revision(), requested) {
            let failure = waiver::supplied_failure(contract, requested, evidence, waiver);
            if let Some(reason) = failure {
                complete = false;
                unmet.push(UnmetCondition::InvalidWaiver { finding_id: waiver.finding_id(), reason });
                assert(unmet@[before.len() as int] == UnmetCondition::InvalidWaiver { finding_id: waiver.spec_finding_id(), reason });
                assert(invalid_waiver_reported(unmet@, waiver.spec_finding_id()));
            }
        }
        proof { diagnostics::preserve_reports(contract, requested, evidence, before, unmet@, index as int); }
        index += 1;
    }
    complete
}

} // verus!
