//! Threat, control, unsafe-code, and trusted-computing-base inventory checks.

use crate::{IntegratedCandidate, InventoryKind, SecurityEvidence, UnmetSecurityCondition};

const fn kinds_equal(left: InventoryKind, right: InventoryKind) -> bool {
    matches!(
        (left, right),
        (InventoryKind::Threats, InventoryKind::Threats)
            | (InventoryKind::Controls, InventoryKind::Controls)
            | (InventoryKind::UnsafeCode, InventoryKind::UnsafeCode)
            | (InventoryKind::TrustedComputingBase, InventoryKind::TrustedComputingBase)
    )
}
fn current_inventory(
    evidence: &SecurityEvidence,
    target: InventoryKind,
    candidate: IntegratedCandidate,
) -> Option<(usize, &crate::InventoryObservation)> {
    let values = evidence.inventories();
    let mut index = 0;
    while index < values.len() {
        if kinds_equal(values[index].kind(), target)
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some((index, &values[index]));
        }
        index += 1;
    }
    None
}
pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < InventoryKind::ALL.len() {
        let kind = InventoryKind::ALL[index];
        match current_inventory(evidence, kind, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingInventory(kind));
            }
            Some((_observation_index, observation)) => {
                if !observation.complete() {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::InventoryIncomplete(kind));
                }
                if !crate::binding::digest_present(observation.evidence_digest()) {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::EmptyInventoryDigest(kind));
                }
            }
        }
        index += 1;
    }
    complete
}
