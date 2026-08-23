//! Concrete structural and accounting predicates for the ledger reducer.

#[cfg(verus_only)]
mod uniqueness;

#[cfg(verus_only)]
pub(crate) use self::uniqueness::{
    matching_accounts_are_unique, matching_reservations_are_unique,
};

#[cfg(verus_only)]
use crate::{
    BudgetAccountPhase, BudgetDimension, BudgetLedger, ReservationPhase, UsageFinality,
};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn optional_digests_equal(
    left: Option<peritus_types::Sha256Digest>,
    right: Option<peritus_types::Sha256Digest>,
) -> bool {
    match (left, right) {
        (Some(left_digest), Some(right_digest)) => {
            crate::identity_model::digests_equal(left_digest, right_digest)
        }
        (None, None) => true,
        _ => false,
    }
}

pub(crate) open spec fn optional_amounts_equal(
    left: Option<crate::BudgetAmounts>,
    right: Option<crate::BudgetAmounts>,
) -> bool {
    match (left, right) {
        (Some(left_amount), Some(right_amount)) => left_amount.spec_equal(right_amount),
        (None, None) => true,
        _ => false,
    }
}

pub(crate) open spec fn optional_finality_is_final(
    value: Option<UsageFinality>,
) -> bool {
    match value {
        Some(UsageFinality::Final) => true,
        _ => false,
    }
}

pub(crate) open spec fn exact_derived_accounting(
    ledger: &BudgetLedger,
    index: int,
) -> bool {
    let account = ledger.accounts[index];
    account.child_delegated_remaining.spec_get(BudgetDimension::ModelTokens)
        == crate::accounting_model::direct_child_remaining_sum(
            ledger,
            account.id,
            BudgetDimension::ModelTokens,
            ledger.accounts@.len() as int,
        )
        && account.child_delegated_remaining.spec_get(
            BudgetDimension::ProviderCostMicrounits,
        ) == crate::accounting_model::direct_child_remaining_sum(
            ledger,
            account.id,
            BudgetDimension::ProviderCostMicrounits,
            ledger.accounts@.len() as int,
        )
        && account.child_delegated_remaining.spec_get(
            BudgetDimension::ActiveEffectMilliseconds,
        ) == crate::accounting_model::direct_child_remaining_sum(
            ledger,
            account.id,
            BudgetDimension::ActiveEffectMilliseconds,
            ledger.accounts@.len() as int,
        )
        && account.child_delegated_remaining.spec_get(BudgetDimension::Attempts)
            == crate::accounting_model::direct_child_remaining_sum(
                ledger,
                account.id,
                BudgetDimension::Attempts,
                ledger.accounts@.len() as int,
            )
        && account.child_delegated_remaining.spec_get(BudgetDimension::Retries)
            == crate::accounting_model::direct_child_remaining_sum(
                ledger,
                account.id,
                BudgetDimension::Retries,
                ledger.accounts@.len() as int,
            )
        && account.operation_reserved.spec_get(BudgetDimension::ModelTokens)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                account.id,
                BudgetDimension::ModelTokens,
                ledger.reservations@.len() as int,
            )
        && account.operation_reserved.spec_get(BudgetDimension::ProviderCostMicrounits)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                account.id,
                BudgetDimension::ProviderCostMicrounits,
                ledger.reservations@.len() as int,
            )
        && account.operation_reserved.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                account.id,
                BudgetDimension::ActiveEffectMilliseconds,
                ledger.reservations@.len() as int,
            )
        && account.operation_reserved.spec_get(BudgetDimension::Attempts)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                account.id,
                BudgetDimension::Attempts,
                ledger.reservations@.len() as int,
            )
        && account.operation_reserved.spec_get(BudgetDimension::Retries)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                account.id,
                BudgetDimension::Retries,
                ledger.reservations@.len() as int,
            )
}

pub(crate) open spec fn record_phase_valid(
    record: crate::state::ReservationRecord,
) -> bool {
    match record.phase {
        ReservationPhase::Held => {
            record.observed.spec_is_zero()
                && record.activation_evidence.is_none()
                && record.observation_evidence.is_none()
                && record.final_evidence.is_none()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::Active => {
            record.activation_evidence.is_some()
                && (record.observed.spec_is_zero() || record.observation_evidence.is_some())
                && record.final_evidence.is_none()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::SettledExact => {
            record.observed.spec_equal(record.request.spec_reserve())
                && record.final_reported.is_none()
                && record.finality.is_none()
                && (record.request.spec_reserve().spec_is_zero()
                    || record.final_evidence.is_some())
        }
        ReservationPhase::SettledFinal => {
            record.activation_evidence.is_some()
                && optional_digests_equal(
                    record.observation_evidence,
                    record.final_evidence,
                )
                && record.final_evidence.is_some()
                && optional_amounts_equal(record.final_reported, Some(record.observed))
                && optional_finality_is_final(record.finality)
        }
        ReservationPhase::CancelledHeld => {
            record.observed.spec_is_zero()
                && record.activation_evidence.is_none()
                && record.observation_evidence.is_none()
                && record.final_evidence.is_some()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::SettledAmbiguous => {
            record.activation_evidence.is_some()
                && record.observed.spec_equal(record.request.spec_reserve())
                && record.final_evidence.is_some()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
        ReservationPhase::OverrunFaulted => {
            record.activation_evidence.is_some()
                && optional_digests_equal(
                    record.observation_evidence,
                    record.final_evidence,
                )
                && record.observed.spec_equal(record.request.spec_reserve())
                && record.final_evidence.is_some()
                && record.finality.is_some()
                && match record.final_reported {
                    Some(reported) => !reported.spec_le(record.request.spec_reserve()),
                    None => false,
                }
        }
    }
}

pub(crate) open spec fn prior_same_action(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    end: int,
) -> bool {
    exists |prior: int| #![auto]
        0 <= prior < end
            && crate::identity_model::revisions_equal(
                ledger.reservations[prior].request.spec_revision(),
                request.spec_revision(),
            )
            && crate::identity_model::action_ids_equal(
                ledger.reservations[prior].request.spec_action_id(),
                request.spec_action_id(),
            )
}

pub(crate) open spec fn prior_exact_request(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    end: int,
) -> bool {
    exists |prior: int| #![auto]
        0 <= prior < end
            && crate::identity_model::revisions_equal(
                ledger.reservations[prior].request.spec_revision(),
                request.spec_revision(),
            )
            && crate::identity_model::action_ids_equal(
                ledger.reservations[prior].request.spec_action_id(),
                request.spec_action_id(),
            )
            && crate::identity_model::digests_equal(
                ledger.reservations[prior].request.spec_action_digest(),
                request.spec_action_digest(),
            )
}

pub(crate) open spec fn prior_history_resolved(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    end: int,
) -> bool {
    forall |prior: int| #![auto]
        0 <= prior < end
            && crate::identity_model::revisions_equal(
                ledger.reservations[prior].request.spec_revision(),
                request.spec_revision(),
            )
            && crate::identity_model::action_ids_equal(
                ledger.reservations[prior].request.spec_action_id(),
                request.spec_action_id(),
            )
            ==> crate::identity_model::digests_equal(
                    ledger.reservations[prior].request.spec_action_digest(),
                    request.spec_action_digest(),
                )
                && !ledger.reservations[prior].phase.spec_is_live()
}

pub(crate) open spec fn attempt_charge_valid(
    request: crate::BudgetRequest,
    retry: bool,
) -> bool {
    request.spec_consume_now().spec_get(BudgetDimension::Attempts) == 1
        && request.spec_consume_now().spec_get(BudgetDimension::Retries)
            == if retry { 1int } else { 0int }
        && request.spec_reserve().spec_get(BudgetDimension::Attempts) == 0
        && request.spec_reserve().spec_get(BudgetDimension::Retries) == 0
}

pub(crate) open spec fn attempt_history_valid(ledger: &BudgetLedger, index: int) -> bool {
    let request = ledger.reservations[index].request;
    attempt_charge_valid(request, prior_exact_request(ledger, request, index))
        && prior_history_resolved(ledger, request, index)
}

pub(crate) open spec fn parent_link_valid(ledger: &BudgetLedger, index: int) -> bool {
    exists |parent: int| #![auto]
        0 <= parent < index
            && crate::identity_model::parent_matches(
                ledger.accounts[index].parent_id,
                ledger.accounts[parent].id,
            )
            && crate::identity_model::revisions_equal(
                ledger.accounts[index].revision,
                ledger.accounts[parent].revision,
            )
}

pub(crate) open spec fn account_unique_before(ledger: &BudgetLedger, index: int) -> bool {
    forall |prior: int| #![auto]
        0 <= prior < index
            ==> !crate::identity_model::budget_ids_equal(
                ledger.accounts[prior].id,
                ledger.accounts[index].id,
            )
}

pub(crate) open spec fn reservation_unique_before(
    ledger: &BudgetLedger,
    index: int,
) -> bool {
    forall |prior: int| #![auto]
        0 <= prior < index
            ==> !crate::identity_model::reservation_ids_equal(
                ledger.reservations[prior].request.spec_reservation_id(),
                ledger.reservations[index].request.spec_reservation_id(),
            )
}

pub(crate) open spec fn closed_account_has_no_live_work(
    ledger: &BudgetLedger,
    index: int,
) -> bool {
    budget_has_no_live_work(ledger, ledger.accounts[index].id)
}

pub(crate) open spec fn budget_has_no_live_work(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> bool {
    (forall |child: int| #![auto]
        0 <= child < ledger.accounts@.len()
            && crate::identity_model::parent_matches(
                ledger.accounts[child].parent_id,
                budget_id,
            )
            ==> ledger.accounts[child].phase == BudgetAccountPhase::Closed)
        && (forall |reservation: int| #![auto]
            0 <= reservation < ledger.reservations@.len()
                && crate::identity_model::budget_ids_equal(
                    ledger.reservations[reservation].request.spec_budget_id(),
                    budget_id,
                )
                ==> !ledger.reservations[reservation].phase.spec_is_live())
}

pub(crate) open spec fn account_entry_valid(ledger: &BudgetLedger, index: int) -> bool {
    account_unique_before(ledger, index)
        && (index == 0 || parent_link_valid(ledger, index))
        && exact_derived_accounting(ledger, index)
        && (ledger.accounts[index].phase != BudgetAccountPhase::Closed
            || closed_account_has_no_live_work(ledger, index))
}

pub(crate) open spec fn reservation_entry_valid(
    ledger: &BudgetLedger,
    index: int,
) -> bool {
    reservation_unique_before(ledger, index)
        && (exists |account: int| #![auto]
            0 <= account < ledger.accounts@.len()
                && crate::identity_model::budget_ids_equal(
                    ledger.reservations[index].request.spec_budget_id(),
                    ledger.accounts[account].id,
                )
                && crate::identity_model::revisions_equal(
                    ledger.reservations[index].request.spec_revision(),
                    ledger.accounts[account].revision,
                ))
        && ledger.reservations[index].observed.spec_le(
            ledger.reservations[index].request.spec_reserve(),
        )
        && record_phase_valid(ledger.reservations[index])
        && attempt_history_valid(ledger, index)
}

pub(crate) open spec fn ledger_structure_holds(ledger: &BudgetLedger) -> bool {
    ledger_account_structure_holds(ledger) && ledger_reservation_structure_holds(ledger)
}

pub(crate) open spec fn ledger_account_structure_holds(ledger: &BudgetLedger) -> bool {
    ledger.accounts@.len() > 0
        && crate::identity_model::budget_ids_equal(ledger.accounts[0].id, ledger.root_id)
        && ledger.accounts[0].parent_id.is_none()
        && (forall |index: int| #![auto]
            0 <= index < ledger.accounts@.len()
                ==> account_entry_valid(ledger, index))
}

pub(crate) open spec fn ledger_reservation_structure_holds(ledger: &BudgetLedger) -> bool {
    forall |index: int| #![auto]
            0 <= index < ledger.reservations@.len()
                ==> reservation_entry_valid(ledger, index)
}

} // verus!
