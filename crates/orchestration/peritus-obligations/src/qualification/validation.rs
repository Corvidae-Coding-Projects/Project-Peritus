//! Verified condition and evidence admission in production error order.

#[cfg(verus_only)]
use super::{admission, model};
use crate::{
    ConditionObservation, ObligationError, ObligationErrorKind, RequirementEvidence,
    RequirementLedger,
};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(super) fn validate_inputs(
    ledger: &RequirementLedger,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> (result: Result<(), ObligationError>)
    ensures
        result.is_ok() == model::qualification_inputs_valid(ledger, conditions@, evidence@),
        match result {
            Ok(()) => true,
            Err(error) => admission::qualification_error(ledger, conditions@, evidence@, &error),
        },
{
    validate_conditions(conditions)?;
    validate_evidence(ledger, evidence)?;
    Ok(())
}

fn validate_conditions(
    conditions: &[ConditionObservation],
) -> (result: Result<(), ObligationError>)
    ensures
        result.is_ok() == crate::order::ordered(model::condition_keys(conditions@)),
        match result {
            Ok(()) => true,
            Err(error) => admission::condition_error(conditions@, &error),
        },
{
    if conditions.len() < 2 {
        assert(crate::order::ordered(model::condition_keys(conditions@)));
        return Ok(());
    }
    let mut index = 1;
    while index < conditions.len()
        invariant
            1 <= index <= conditions@.len(),
            forall |prior: int| 1 <= prior < index ==>
                crate::order::byte_order(
                    #[trigger] model::condition_keys(conditions@)[prior - 1],
                    #[trigger] model::condition_keys(conditions@)[prior],
                ) == Ordering::Less,
        decreases conditions.len() - index,
    {
        let order = crate::order::compare(
            conditions[index - 1].condition_id().digest().as_bytes(),
            conditions[index].condition_id().digest().as_bytes(),
        );
        assert(order == crate::order::byte_order(
            model::condition_keys(conditions@)[index as int - 1],
            model::condition_keys(conditions@)[index as int],
        ));
        match order {
            Ordering::Equal => {
                assert(admission::first_order_failure(model::condition_keys(conditions@), index as int));
                assert(!crate::order::ordered(model::condition_keys(conditions@))) by {
                    if crate::order::ordered(model::condition_keys(conditions@)) {
                        model::ordered_at(model::condition_keys(conditions@), index as int);
                    }
                }
                return Err(ObligationError::plain(ObligationErrorKind::InvalidCondition));
            },
            Ordering::Greater => {
                assert(admission::first_order_failure(model::condition_keys(conditions@), index as int));
                assert(!crate::order::ordered(model::condition_keys(conditions@))) by {
                    if crate::order::ordered(model::condition_keys(conditions@)) {
                        model::ordered_at(model::condition_keys(conditions@), index as int);
                    }
                }
                return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
            },
            Ordering::Less => {},
        }
        index += 1;
    }
    assert(crate::order::ordered(model::condition_keys(conditions@)));
    Ok(())
}

fn validate_evidence(
    ledger: &RequirementLedger,
    evidence: &[RequirementEvidence],
) -> (result: Result<(), ObligationError>)
    ensures
        result.is_ok() == (
            evidence@.len() <= ledger.spec_limits().spec_max_evidence()
            && crate::order::ordered(model::evidence_keys(evidence@))
            && model::all_evidence_known(ledger, evidence@)
        ),
        match result {
            Ok(()) => true,
            Err(error) => admission::evidence_error(ledger, evidence@, &error),
        },
{
    if evidence.len() > ledger.limits().max_evidence() {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            ledger.limits().max_evidence() as u64,
            evidence.len() as u64,
        ));
    }
    if evidence.len() < 2 {
        assert(crate::order::ordered(model::evidence_keys(evidence@)));
    }
    let mut index = 1;
    while index < evidence.len()
        invariant
            evidence@.len() <= ledger.spec_limits().spec_max_evidence(),
            1 <= index,
            evidence@.len() < 2 || index <= evidence@.len(),
            forall |prior: int| 1 <= prior < index ==>
                crate::order::byte_order(
                    #[trigger] model::evidence_keys(evidence@)[prior - 1],
                    #[trigger] model::evidence_keys(evidence@)[prior],
                ) == Ordering::Less,
        decreases evidence.len() - index,
    {
        let order = crate::order::compare(
            evidence[index - 1].requirement_id().digest().as_bytes(),
            evidence[index].requirement_id().digest().as_bytes(),
        );
        assert(order == crate::order::byte_order(
            model::evidence_keys(evidence@)[index as int - 1],
            model::evidence_keys(evidence@)[index as int],
        ));
        match order {
            Ordering::Equal => {
                assert(admission::first_order_failure(model::evidence_keys(evidence@), index as int));
                assert(!crate::order::ordered(model::evidence_keys(evidence@))) by {
                    if crate::order::ordered(model::evidence_keys(evidence@)) {
                        model::ordered_at(model::evidence_keys(evidence@), index as int);
                    }
                }
                return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
            },
            Ordering::Greater => {
                assert(admission::first_order_failure(model::evidence_keys(evidence@), index as int));
                assert(!crate::order::ordered(model::evidence_keys(evidence@))) by {
                    if crate::order::ordered(model::evidence_keys(evidence@)) {
                        model::ordered_at(model::evidence_keys(evidence@), index as int);
                    }
                }
                return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
            },
            Ordering::Less => {},
        }
        index += 1;
    }
    assert(crate::order::ordered(model::evidence_keys(evidence@)));

    index = 0;
    while index < evidence.len()
        invariant
            evidence@.len() <= ledger.spec_limits().spec_max_evidence(),
            crate::order::ordered(model::evidence_keys(evidence@)),
            index <= evidence@.len(),
            forall |prior: int| 0 <= prior < index ==>
                model::ledger_contains(
                    ledger.spec_entries(),
                    #[trigger] evidence@[prior].spec_binding().spec_requirement_id(),
                ),
        decreases evidence.len() - index,
    {
        let id = evidence[index].requirement_id();
        match ledger.entry(id) {
            None => {
                assert(!model::ledger_contains(ledger.spec_entries(), id));
                assert(!model::all_evidence_known(ledger, evidence@)) by {
                    if model::all_evidence_known(ledger, evidence@) {
                        assert(model::ledger_contains(
                            ledger.spec_entries(),
                            evidence@[index as int].spec_binding().spec_requirement_id(),
                        ));
                    }
                }
                return Err(ObligationError::requirement(
                    ObligationErrorKind::UnknownRequirement,
                    id,
                ));
            },
            Some(_entry) => {
                assert(model::requirement_key(_entry.spec_id()) == model::requirement_key(id));
                assert(model::ledger_contains(ledger.spec_entries(), id)) by {
                    let witness = choose |witness: int| 0 <= witness < ledger.spec_entries().len()
                        && #[trigger] ledger.spec_entries()[witness].spec_same_content(_entry);
                    assert(model::requirement_key(ledger.spec_entries()[witness].spec_id())
                        == model::requirement_key(_entry.spec_id()));
                };
            },
        }
        index += 1;
    }
    assert(model::all_evidence_known(ledger, evidence@));
    Ok(())
}

} // verus!
