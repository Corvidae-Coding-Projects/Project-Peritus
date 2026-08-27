//! Threat, control, unsafe-code, and trusted-computing-base inventory checks.

use crate::{IntegratedCandidate, InventoryKind, SecurityEvidence, UnmetSecurityCondition};
use vstd::prelude::*;

verus! {

fn current_inventory(
    evidence: &SecurityEvidence,
    target: InventoryKind,
    candidate: IntegratedCandidate,
) -> Option<&crate::InventoryObservation> {
    let values = evidence.inventories();
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].kind() == target
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some(&values[index]);
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
    while index < InventoryKind::ALL.len()
        invariant 0 <= index <= InventoryKind::ALL.len(),
        decreases InventoryKind::ALL.len() - index,
    {
        let kind = InventoryKind::ALL[index];
        match current_inventory(evidence, kind, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingInventory(kind));
            }
            Some(observation) => {
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

} // verus!
