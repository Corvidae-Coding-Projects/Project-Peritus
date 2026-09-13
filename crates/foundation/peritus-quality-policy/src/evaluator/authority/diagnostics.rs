//! Monotonic preservation of already emitted waiver diagnostics.

#[cfg(verus_only)]
use crate::model::authority::*;
#[cfg(verus_only)]
use crate::{AcceptanceEvidence, UnmetCondition};
#[cfg(verus_only)]
use peritus_spec::AcceptanceContract;
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

pub open spec fn preserved(before: Seq<UnmetCondition>, after: Seq<UnmetCondition>) -> bool {
    before.len() <= after.len() && forall |index: int| 0 <= index < before.len() ==> #[trigger] after[index] == before[index]
}

pub proof fn preserve_reports(contract: &AcceptanceContract, revision: RevisionTuple, evidence: &AcceptanceEvidence, before: Seq<UnmetCondition>, after: Seq<UnmetCondition>, end: int)
    requires
        preserved(before, after),
        forall |index: int| 0 <= index < end && index < evidence.spec_waivers().len()
            && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[index].spec_revision(), revision)
            && !supplied_waiver_valid(contract, revision, evidence, evidence.spec_waivers()[index])
            ==> invalid_waiver_reported(before, evidence.spec_waivers()[index].spec_finding_id()),
    ensures
        forall |index: int| 0 <= index < end && index < evidence.spec_waivers().len()
            && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[index].spec_revision(), revision)
            && !supplied_waiver_valid(contract, revision, evidence, evidence.spec_waivers()[index])
            ==> invalid_waiver_reported(after, evidence.spec_waivers()[index].spec_finding_id()),
{
    assert forall |index: int| 0 <= index < end && index < evidence.spec_waivers().len()
        && crate::model::revision_fresh(#[trigger] evidence.spec_waivers()[index].spec_revision(), revision)
        && !supplied_waiver_valid(contract, revision, evidence, evidence.spec_waivers()[index])
        implies invalid_waiver_reported(after, evidence.spec_waivers()[index].spec_finding_id()) by {
        let finding = evidence.spec_waivers()[index].spec_finding_id();
        let at = choose |at: int| 0 <= at < before.len() && match #[trigger] before[at] {
            UnmetCondition::InvalidWaiver { finding_id, .. } => finding_ids_match(finding_id, finding),
            _ => false,
        };
        assert(after[at] == before[at]);
    }
}

} // verus!
