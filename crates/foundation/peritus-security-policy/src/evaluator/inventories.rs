//! Threat, control, unsafe-code, and trusted-computing-base inventory checks.

mod model;

use crate::{IntegratedCandidate, InventoryKind, SecurityEvidence, UnmetSecurityCondition};
use vstd::prelude::*;

verus! {

pub open spec fn inventories_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::complete(evidence, candidate)
}

const fn kinds_equal(left: InventoryKind, right: InventoryKind) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
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
) -> (result: Option<(usize, &crate::InventoryObservation)>)
    ensures match result {
        Some((index, observation)) => {
            &&& model::first_observation_at(
                evidence.spec_inventories(), index as int, target, candidate)
            &&& *observation == evidence.spec_inventories()[index as int]
        },
        None => forall |index: int| 0 <= index < evidence.spec_inventories().len() ==>
            !model::observation_matches(
                #[trigger] evidence.spec_inventories()[index], target, candidate),
    },
{
    let values = evidence.inventories();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_inventories(),
            forall |prior: int| 0 <= prior < index ==>
                !model::observation_matches(#[trigger] values@[prior], target, candidate),
        decreases values.len() - index,
    {
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
) -> (complete: bool)
    ensures
        complete == inventories_complete(evidence, candidate),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut index = 0;
    while index < InventoryKind::ALL.len()
        invariant
            0 <= index <= InventoryKind::ALL.len(),
            complete == model::complete_through(evidence, candidate, index as int),
            complete ==> unmet@ == old(unmet)@,
        decreases InventoryKind::ALL.len() - index,
    {
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
                proof {
                    model::admission_at_first(
                        evidence.spec_inventories(),
                        _observation_index as int,
                        kind,
                        candidate,
                    );
                };
            }
        }
        proof {
            model::complete_step(evidence, candidate, index as int);
        }
        index += 1;
    }
    reveal(inventories_complete);
    reveal(model::complete);
    complete
}

} // verus!
