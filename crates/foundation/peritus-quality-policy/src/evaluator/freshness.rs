//! INV-003 executable freshness checks.

use crate::{
    AcceptanceEvidence, ApprovalObservation, EvidenceObservation, GateObservation,
    ObservationKind, ReviewObservation, UnmetCondition, WaiverObservation,
};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

fn gates_current(values: &[GateObservation], requested: RevisionTuple) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < values@.len() ==>
        #[trigger] crate::model::revision_fresh(values@[index].spec_revision(), requested)),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::model::revision_fresh(values@[prior].spec_revision(), requested),
        decreases values.len() - index,
    {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            assert(!(forall |prior: int| 0 <= prior < values@.len() ==>
                #[trigger] crate::model::revision_fresh(
                    values@[prior].spec_revision(), requested))) by {
                assert(!crate::model::revision_fresh(values@[index as int].spec_revision(), requested));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn reviews_current(values: &[ReviewObservation], requested: RevisionTuple) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < values@.len() ==>
        #[trigger] crate::model::revision_fresh(values@[index].spec_revision(), requested)),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::model::revision_fresh(values@[prior].spec_revision(), requested),
        decreases values.len() - index,
    {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            assert(!(forall |prior: int| 0 <= prior < values@.len() ==>
                #[trigger] crate::model::revision_fresh(
                    values@[prior].spec_revision(), requested))) by {
                assert(!crate::model::revision_fresh(values@[index as int].spec_revision(), requested));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn evidence_current(values: &[EvidenceObservation], requested: RevisionTuple) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < values@.len() ==>
        #[trigger] crate::model::revision_fresh(values@[index].spec_revision(), requested)),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::model::revision_fresh(values@[prior].spec_revision(), requested),
        decreases values.len() - index,
    {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            assert(!(forall |prior: int| 0 <= prior < values@.len() ==>
                #[trigger] crate::model::revision_fresh(
                    values@[prior].spec_revision(), requested))) by {
                assert(!crate::model::revision_fresh(values@[index as int].spec_revision(), requested));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn approvals_current(values: &[ApprovalObservation], requested: RevisionTuple) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < values@.len() ==>
        #[trigger] crate::model::revision_fresh(values@[index].spec_revision(), requested)),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::model::revision_fresh(values@[prior].spec_revision(), requested),
        decreases values.len() - index,
    {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            assert(!(forall |prior: int| 0 <= prior < values@.len() ==>
                #[trigger] crate::model::revision_fresh(
                    values@[prior].spec_revision(), requested))) by {
                assert(!crate::model::revision_fresh(values@[index as int].spec_revision(), requested));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn waivers_current(values: &[WaiverObservation], requested: RevisionTuple) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < values@.len() ==>
        #[trigger] crate::model::revision_fresh(values@[index].spec_revision(), requested)),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::model::revision_fresh(values@[prior].spec_revision(), requested),
        decreases values.len() - index,
    {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            assert(!(forall |prior: int| 0 <= prior < values@.len() ==>
                #[trigger] crate::model::revision_fresh(
                    values@[prior].spec_revision(), requested))) by {
                assert(!crate::model::revision_fresh(values@[index as int].spec_revision(), requested));
            };
            return false;
        }
        index += 1;
    }
    true
}

pub(super) fn evaluate(
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    unmet: &mut Vec<UnmetCondition>,
) -> (fresh: bool)
    ensures fresh == evidence.spec_all_current(requested),
{
    let mut index = 0;
    while index < evidence.gates().len()
        invariant 0 <= index <= evidence.spec_gates().len(),
        decreases evidence.spec_gates().len() - index,
    {
        if !crate::revision::revision_matches(evidence.gates()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Gate, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.reviews().len()
        invariant 0 <= index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - index,
    {
        if !crate::revision::revision_matches(evidence.reviews()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Review, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.evidence().len()
        invariant 0 <= index <= evidence.spec_evidence().len(),
        decreases evidence.spec_evidence().len() - index,
    {
        if !crate::revision::revision_matches(evidence.evidence()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Evidence, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.approvals().len()
        invariant 0 <= index <= evidence.spec_approvals().len(),
        decreases evidence.spec_approvals().len() - index,
    {
        if !crate::revision::revision_matches(evidence.approvals()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Approval, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.waivers().len()
        invariant 0 <= index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - index,
    {
        if !crate::revision::revision_matches(evidence.waivers()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Waiver, index });
        }
        index += 1;
    }

    gates_current(evidence.gates(), requested)
        && reviews_current(evidence.reviews(), requested)
        && evidence_current(evidence.evidence(), requested)
        && approvals_current(evidence.approvals(), requested)
        && waivers_current(evidence.waivers(), requested)
}

} // verus!
