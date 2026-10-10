//! INV-003 executable freshness checks.

use crate::{
    AcceptanceEvidence, ApprovalObservation, EvidenceObservation, GateObservation, ObservationKind,
    ReviewObservation, UnmetCondition, WaiverObservation,
};
use peritus_types::RevisionTuple;

fn gates_current(values: &[GateObservation], requested: RevisionTuple) -> bool {
    let mut index = 0;
    while index < values.len() {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            return false;
        }
        index += 1;
    }
    true
}
fn reviews_current(values: &[ReviewObservation], requested: RevisionTuple) -> bool {
    let mut index = 0;
    while index < values.len() {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            return false;
        }
        index += 1;
    }
    true
}
fn evidence_current(values: &[EvidenceObservation], requested: RevisionTuple) -> bool {
    let mut index = 0;
    while index < values.len() {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            return false;
        }
        index += 1;
    }
    true
}
fn approvals_current(values: &[ApprovalObservation], requested: RevisionTuple) -> bool {
    let mut index = 0;
    while index < values.len() {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
            return false;
        }
        index += 1;
    }
    true
}
fn waivers_current(values: &[WaiverObservation], requested: RevisionTuple) -> bool {
    let mut index = 0;
    while index < values.len() {
        if !crate::revision::revision_matches(values[index].revision(), requested) {
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
) -> bool {
    let mut index = 0;
    while index < evidence.gates().len() {
        if !crate::revision::revision_matches(evidence.gates()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Gate, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.reviews().len() {
        if !crate::revision::revision_matches(evidence.reviews()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Review, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.evidence().len() {
        if !crate::revision::revision_matches(evidence.evidence()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Evidence, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.approvals().len() {
        if !crate::revision::revision_matches(evidence.approvals()[index].revision(), requested) {
            unmet.push(UnmetCondition::StaleObservation { kind: ObservationKind::Approval, index });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.waivers().len() {
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
