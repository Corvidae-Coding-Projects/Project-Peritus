//! Exact first-error contracts for the public qualification boundary.

use super::model;
use crate::{
    ConditionObservation, ObligationError, ObligationErrorKind, RequirementEvidence,
    RequirementLedger,
};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// First adjacent pair that is not strictly increasing.
pub open spec fn first_order_failure(keys: Seq<Seq<u8>>, index: int) -> bool {
    &&& 1 <= index < keys.len()
    &&& crate::order::byte_order(keys[index - 1], keys[index]) != Ordering::Less
    &&& forall |prior: int| 1 <= prior < index ==>
        crate::order::byte_order(#[trigger] keys[prior - 1], keys[prior]) == Ordering::Less
}

/// Complete error for the first conflicting or descending condition pair.
pub open spec fn condition_error(
    conditions: Seq<ConditionObservation>,
    error: &ObligationError,
) -> bool {
    let keys = model::condition_keys(conditions);
    exists |index: int| #[trigger] first_order_failure(keys, index)
        && match crate::order::byte_order(keys[index - 1], keys[index]) {
            Ordering::Equal => error.spec_plain(ObligationErrorKind::InvalidCondition),
            Ordering::Greater => error.spec_plain(ObligationErrorKind::NonCanonicalOrder),
            Ordering::Less => false,
        }
}

/// Complete evidence error, preserving bound, order, then unknown-identity precedence.
pub open spec fn evidence_error(
    ledger: &RequirementLedger,
    evidence: Seq<RequirementEvidence>,
    error: &ObligationError,
) -> bool {
    let keys = model::evidence_keys(evidence);
    if evidence.len() > ledger.spec_limits().spec_max_evidence() {
        error.spec_numbers(
            ObligationErrorKind::LimitExceeded,
            ledger.spec_limits().spec_max_evidence() as u64,
            evidence.len() as u64,
        )
    } else if !crate::order::ordered(keys) {
        exists |index: int| #[trigger] first_order_failure(keys, index)
            && match crate::order::byte_order(keys[index - 1], keys[index]) {
                Ordering::Equal => error.spec_plain(ObligationErrorKind::DuplicateValue),
                Ordering::Greater => error.spec_plain(ObligationErrorKind::NonCanonicalOrder),
                Ordering::Less => false,
            }
    } else {
        exists |index: int| 0 <= index < evidence.len()
            && !model::ledger_contains(
                ledger.spec_entries(),
                #[trigger] evidence[index].spec_binding().spec_requirement_id(),
            )
            && (forall |prior: int| 0 <= prior < index ==>
                model::ledger_contains(
                    ledger.spec_entries(),
                    #[trigger] evidence[prior].spec_binding().spec_requirement_id(),
                ))
            && error.spec_requirement(
                ObligationErrorKind::UnknownRequirement,
                evidence[index].spec_binding().spec_requirement_id(),
            )
    }
}

/// Complete qualification error, with condition validation preceding all evidence checks.
pub open spec fn qualification_error(
    ledger: &RequirementLedger,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
    error: &ObligationError,
) -> bool {
    if !crate::order::ordered(model::condition_keys(conditions)) {
        condition_error(conditions, error)
    } else {
        evidence_error(ledger, evidence, error)
    }
}

} // verus!
