//! Total value-in/value-out command reducer.

#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::large_types_passed_by_value,
    clippy::manual_let_else,
    clippy::match_like_matches_macro,
    clippy::missing_const_for_fn,
    clippy::only_used_in_recursion,
    clippy::option_if_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::unnecessary_cast,
    reason = "Verus reducer contracts and ghost blocks require syntax ordinary Clippy measures after proof erasure"
)]

use crate::{BudgetCommand, BudgetError, BudgetLedger, BudgetSnapshot, ReservationSnapshot};
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

mod accounting;
mod allocation;
mod lifecycle;
mod reconciliation;
mod reducer;
mod validation;

verus! {

#[allow(clippy::large_types_passed_by_value)]
// Crate visibility prevents this proof contract from exposing private ledger representation.
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn apply(
    ledger: &BudgetLedger,
    command: BudgetCommand,
) -> (result: Result<crate::BudgetTransition, BudgetError>)
    ensures
        ledger.spec_transition_result(command, result),
        match result {
            Ok(transition) =>
                crate::reachability::accepted_step(
                    ledger,
                    command,
                    &transition.spec_ledger(),
                    transition.spec_receipt(),
                )
                    && crate::reachability::budget_step(
                        ledger,
                        command,
                        crate::reachability::BudgetStepOutcome::Accepted(
                            transition.spec_ledger(),
                            transition.spec_receipt(),
                        ),
                    )
                    && crate::model::ledger_well_formed(&transition.spec_ledger())
                    && crate::model::ledger_consumption_monotonic(
                        ledger,
                        &transition.spec_ledger(),
                    )
                    && crate::model::ledger_high_water_monotonic(
                        ledger,
                        &transition.spec_ledger(),
                    )
                    && crate::refinement_model::ledger_identity_stable(
                        ledger,
                        &transition.spec_ledger(),
                    )
                    && crate::refinement_model::ancestor_consumption_propagates(
                        ledger,
                        &transition.spec_ledger(),
                    ),
            // `ledger` is immutably borrowed, and an error contains no candidate next state.
            Err(error) => crate::reachability::rejected_step(
                ledger,
                command,
                error,
                ledger,
            ) && crate::reachability::budget_step(
                ledger,
                command,
                crate::reachability::BudgetStepOutcome::Rejected(*ledger, error),
            ),
        },
{
    let input_validation = validation::validate(ledger);
    if input_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        proof {
            assert(!crate::model::ledger_well_formed(ledger));
            assert(crate::reachability::exact_corrupt_root(ledger, error));
            crate::reachability::malformed_input_rejects(ledger, command, error);
            crate::reachability::rejected_is_budget_step(
                ledger,
                command,
                error,
            );
            ledger.rejected_result_is_exact(command, error);
        }
        return Err(error);
    }
    assert(crate::model::ledger_well_formed(ledger));
    let mut next = ledger.duplicate();
    let ghost reducer_input = next;
    proof {
        assert(crate::reachability::exact_input_view(ledger, &next));
        crate::reachability::exact_input_view_preserves_well_formed(ledger, &next);
    }
    let reducer_result = reducer::apply_validated(&mut next, command);
    let receipt = match reducer_result {
        Ok(receipt) => receipt,
        Err(error) => {
            proof {
                crate::reachability::duplicate_input_refines_rejection_cause(
                    ledger,
                    &reducer_input,
                    command,
                    error,
                );
                crate::reachability::semantic_error_rejects(ledger, command, error);
                crate::reachability::rejected_is_budget_step(
                    ledger,
                    command,
                    error,
                );
                ledger.rejected_result_is_exact(command, error);
            }
            return Err(error);
        }
    };
    assert(crate::reachability::candidate_step(
        &reducer_input,
        command,
        &next,
        receipt,
    ));
    proof {
        crate::reachability::duplicate_input_refines_accepted_step(
            ledger,
            &reducer_input,
            command,
            &next,
            receipt,
        );
    }
    let output_validation = validation::validate(&next);
    if output_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        proof {
            assert(!crate::model::ledger_well_formed(&next));
            assert(crate::reachability::exact_corrupt_root(ledger, error));
            crate::reachability::invalid_successor_rejects(
                ledger,
                command,
                &next,
                receipt,
                error,
            );
            crate::reachability::rejected_is_budget_step(
                ledger,
                command,
                error,
            );
            ledger.rejected_result_is_exact(command, error);
        }
        return Err(error);
    }
    assert(crate::model::ledger_well_formed(&next));
    let refinement_validation = validation::validate_refinement(ledger, &next);
    if refinement_validation.is_err() {
        let error = crate::model::corrupt(ledger.root_id);
        proof {
            assert(!crate::reachability::complete_refinement(ledger, &next));
            assert(crate::reachability::exact_corrupt_root(ledger, error));
            crate::reachability::invalid_refinement_rejects(
                ledger,
                command,
                &next,
                receipt,
                error,
            );
            crate::reachability::rejected_is_budget_step(
                ledger,
                command,
                error,
            );
            ledger.rejected_result_is_exact(command, error);
        }
        return Err(error);
    }
    assert(crate::reachability::complete_refinement(ledger, &next));
    let ghost reducer_output = next;
    let ghost reducer_receipt = receipt;
    assert(crate::reachability::candidate_step(
        ledger,
        command,
        &reducer_output,
        reducer_receipt,
    ));
    assert(crate::model::ledger_well_formed(&reducer_output));
    assert(crate::reachability::complete_refinement(ledger, &reducer_output));
    proof {
        crate::reachability::validated_candidate_is_accepted(
            ledger, command, &reducer_output, reducer_receipt,
        );
    }
    let transition = crate::BudgetTransition::new(next, receipt);
    proof {
        assert(transition.spec_ledger() == reducer_output);
        assert(transition.spec_receipt() == reducer_receipt);
        assert(crate::reachability::accepted_step(
            ledger,
            command,
            &transition.spec_ledger(),
            transition.spec_receipt(),
        ));
        assert(crate::model::ledger_well_formed(&transition.spec_ledger()));
        crate::reachability::accepted_well_formed_is_budget_step(
            ledger,
            command,
            &transition.spec_ledger(),
            transition.spec_receipt(),
        );
        ledger.accepted_result_is_exact(command, transition);
        assert(crate::model::ledger_consumption_monotonic(
            ledger,
            &transition.spec_ledger(),
        ));
        assert(crate::model::ledger_high_water_monotonic(
            ledger,
            &transition.spec_ledger(),
        ));
        assert(crate::refinement_model::ledger_identity_stable(
            ledger,
            &transition.spec_ledger(),
        ));
        assert(crate::refinement_model::ancestor_consumption_propagates(
            ledger,
            &transition.spec_ledger(),
        ));
    }
    Ok(transition)
}

pub fn snapshot_account(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<BudgetSnapshot, BudgetError>)
    ensures ledger.spec_account_result(budget_id, result),
{
    match validation::validate(ledger) {
        Ok(()) => {}
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            proof {
                assert(!crate::model::ledger_well_formed(ledger));
                ledger.account_corrupt_result(budget_id, error);
            }
            return Err(error);
        }
    }
    let index = match accounting::find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            let error = BudgetError::budget(
                crate::BudgetErrorKind::UnknownBudget,
                budget_id,
            );
            proof { ledger.account_unknown_result(budget_id, error); }
            return Err(error);
        }
    };
    let account = &ledger.accounts[index];
    let available = match crate::model::available(account) {
        Ok(available) => available,
        Err(_) => {
            assert(false);
            return Err(crate::model::corrupt(ledger.root_id));
        }
    };
    let snapshot = BudgetSnapshot::from_account(account, available);
    proof {
        ledger.account_snapshot_result(budget_id, index as int, available, snapshot);
    }
    Ok(snapshot)
}

pub fn snapshot_reservation(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> (result: Result<ReservationSnapshot, BudgetError>)
    ensures ledger.spec_reservation_result(reservation_id, result),
{
    match validation::validate(ledger) {
        Ok(()) => {}
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            proof {
                assert(!crate::model::ledger_well_formed(ledger));
                ledger.reservation_corrupt_result(reservation_id, error);
            }
            return Err(error);
        }
    }
    let index = match accounting::find_reservation(ledger, reservation_id) {
        Some(index) => index,
        None => {
            let error = BudgetError::reservation(
                crate::BudgetErrorKind::UnknownReservation,
                reservation_id,
            );
            proof { ledger.reservation_unknown_result(reservation_id, error); }
            return Err(error);
        }
    };
    let record = &ledger.reservations[index];
    proof {
        assert(crate::model::ledger_well_formed(ledger));
        assert(crate::invariant::ledger_structure_holds(ledger));
        assert(crate::invariant::ledger_reservation_structure_holds(ledger));
        assert(crate::invariant::reservation_entry_valid(ledger, index as int));
        assert(record.observed.spec_le(record.request.spec_reserve()));
    }
    let outstanding = match accounting::outstanding(record) {
        Ok(outstanding) => outstanding,
        Err(_) => {
            assert(false);
            return Err(crate::model::corrupt(ledger.root_id));
        }
    };
    let snapshot = ReservationSnapshot::from_record(record, outstanding);
    proof {
        ledger.reservation_snapshot_result(
            reservation_id,
            index as int,
            outstanding,
            snapshot,
        );
    }
    Ok(snapshot)
}

pub fn validate(ledger: &BudgetLedger) -> (result: Result<(), BudgetError>)
    ensures ledger.spec_validation_result(result),
{
    match validation::validate(ledger) {
        Ok(()) => {
            proof { ledger.valid_validation_result(); }
            Ok(())
        }
        Err(_) => {
            let error = crate::model::corrupt(ledger.root_id);
            proof {
                assert(!crate::model::ledger_well_formed(ledger));
                ledger.corrupt_validation_result(error);
            }
            Err(error)
        }
    }
}

} // verus!
