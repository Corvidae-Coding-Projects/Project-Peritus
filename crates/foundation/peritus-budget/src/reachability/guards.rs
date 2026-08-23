//! Exact input-state guards for every accepted command outcome.

#[cfg(verus_only)]
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetCommand, BudgetLedger, BudgetReceiptKind,
    ReservationPhase,
};
#[cfg(verus_only)]
use peritus_types::{BudgetId, BudgetReservationId};
use vstd::prelude::*;

mod operations;
mod outcomes;
mod lineage_proofs;

#[cfg(verus_only)]
pub(crate) use operations::{
    activation_guard_from_runtime, allocate_guard, allocate_guard_from_runtime, begin_guard,
    begin_guard_from_runtime,
};
#[cfg(verus_only)]
pub(crate) use outcomes::{
    cancellation_guard, cancellation_guard_from_runtime, finalization_guard_from_runtime,
    full_finalization_guard, lifecycle_guard_from_runtime, observation_guard,
    observation_guard_from_runtime,
    terminal_observation_matches,
};
#[cfg(verus_only)]
pub(crate) use lineage_proofs::{
    absent_account_has_no_lineage, account_without_chain_has_no_lineage,
    non_open_head_has_no_chain, open_chain_from_parent, open_chain_implies_parent_chain,
    open_chain_root,
};

verus! {

pub(crate) open spec fn account_at(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    index: int,
) -> bool {
    0 <= index < ledger.accounts@.len()
        && crate::identity_model::budget_ids_equal(ledger.accounts[index].id, budget_id)
}

pub(crate) open spec fn reservation_at(
    ledger: &BudgetLedger,
    reservation_id: BudgetReservationId,
    index: int,
) -> bool {
    0 <= index < ledger.reservations@.len()
        && crate::identity_model::reservation_ids_equal(
            ledger.reservations[index].request.spec_reservation_id(),
            reservation_id,
        )
}

pub(crate) open spec fn reference_binding(
    record: crate::state::ReservationRecord,
    reference: crate::ReservationReference,
) -> bool {
    crate::identity_model::action_ids_equal(
        record.request.spec_action_id(),
        reference.spec_action_id(),
    ) && crate::identity_model::digests_equal(
        record.request.spec_action_digest(),
        reference.spec_action_digest(),
    )
}

pub(crate) open spec fn activation_binding(
    record: crate::state::ReservationRecord,
    activation: crate::Activation,
) -> bool {
    crate::identity_model::action_ids_equal(
        record.request.spec_action_id(),
        activation.spec_action_id(),
    ) && crate::identity_model::digests_equal(
        record.request.spec_action_digest(),
        activation.spec_action_digest(),
    )
}

pub(crate) open spec fn observation_binding(
    record: crate::state::ReservationRecord,
    observation: crate::UsageObservation,
) -> bool {
    crate::identity_model::action_ids_equal(
        record.request.spec_action_id(),
        observation.spec_action_id(),
    ) && crate::identity_model::digests_equal(
        record.request.spec_action_digest(),
        observation.spec_action_digest(),
    )
}

pub(crate) open spec fn open_parent_chain(ledger: &BudgetLedger, index: int) -> bool {
    exists |path: Seq<int>| #![auto] open_lineage_path(ledger, index, path)
}

pub(crate) open spec fn open_lineage_path(
    ledger: &BudgetLedger,
    index: int,
    path: Seq<int>,
) -> bool {
    path.len() > 0
        && path[0] == index
        && (forall |position: int| #![auto]
            0 <= position < path.len()
                ==> 0 <= path[position] < ledger.accounts@.len()
                    && ledger.accounts[path[position]].phase == BudgetAccountPhase::Open)
        && (forall |position: int| #![auto]
            0 <= position && position + 1 < path.len()
                ==> 0 <= path[position + 1] < path[position]
                    && crate::identity_model::parent_matches(
                        ledger.accounts[path[position]].parent_id,
                        ledger.accounts[path[position + 1]].id,
                    ))
        && ledger.accounts[path[path.len() - 1]].parent_id.is_none()
}

pub(crate) open spec fn lineage_is_open(ledger: &BudgetLedger, budget_id: BudgetId) -> bool {
    exists |target: int| #![auto]
        account_at(ledger, budget_id, target) && open_parent_chain(ledger, target)
}

pub(crate) open spec fn dimension_capacity_fits(
    account: crate::state::BudgetAccount,
    requested: BudgetAmounts,
    dimension: crate::BudgetDimension,
) -> bool {
    account.consumed.spec_get(dimension)
        + account.operation_reserved.spec_get(dimension)
        + account.child_delegated_remaining.spec_get(dimension)
        + requested.spec_get(dimension)
        <= account.limits.spec_amounts().spec_get(dimension)
}

pub(crate) open spec fn capacity_fits(
    account: crate::state::BudgetAccount,
    requested: BudgetAmounts,
) -> bool {
    dimension_capacity_fits(account, requested, crate::BudgetDimension::ModelTokens)
        && dimension_capacity_fits(
            account,
            requested,
            crate::BudgetDimension::ProviderCostMicrounits,
        )
        && dimension_capacity_fits(
            account,
            requested,
            crate::BudgetDimension::ActiveEffectMilliseconds,
        )
        && dimension_capacity_fits(account, requested, crate::BudgetDimension::Attempts)
        && dimension_capacity_fits(account, requested, crate::BudgetDimension::Retries)
}

pub(crate) open spec fn request_capacity_fits(
    account: crate::state::BudgetAccount,
    request: crate::BudgetRequest,
) -> bool {
    request.spec_consume_now().spec_get(crate::BudgetDimension::ModelTokens)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ModelTokens)
            + account.consumed.spec_get(crate::BudgetDimension::ModelTokens)
            + account.operation_reserved.spec_get(crate::BudgetDimension::ModelTokens)
            + account.child_delegated_remaining.spec_get(crate::BudgetDimension::ModelTokens)
        <= account.limits.spec_amounts().spec_get(crate::BudgetDimension::ModelTokens)
        && request.spec_consume_now().spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + account.consumed.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + account.operation_reserved.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + account.child_delegated_remaining.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
        <= account.limits.spec_amounts().spec_get(crate::BudgetDimension::ProviderCostMicrounits)
        && request.spec_consume_now().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + account.consumed.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + account.operation_reserved.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + account.child_delegated_remaining.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
        <= account.limits.spec_amounts().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
        && request.spec_consume_now().spec_get(crate::BudgetDimension::Attempts)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Attempts)
            + account.consumed.spec_get(crate::BudgetDimension::Attempts)
            + account.operation_reserved.spec_get(crate::BudgetDimension::Attempts)
            + account.child_delegated_remaining.spec_get(crate::BudgetDimension::Attempts)
        <= account.limits.spec_amounts().spec_get(crate::BudgetDimension::Attempts)
        && request.spec_consume_now().spec_get(crate::BudgetDimension::Retries)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Retries)
            + account.consumed.spec_get(crate::BudgetDimension::Retries)
            + account.operation_reserved.spec_get(crate::BudgetDimension::Retries)
            + account.child_delegated_remaining.spec_get(crate::BudgetDimension::Retries)
        <= account.limits.spec_amounts().spec_get(crate::BudgetDimension::Retries)
}

pub(crate) proof fn capacity_from_available(
    account: crate::state::BudgetAccount,
    requested: BudgetAmounts,
    available: BudgetAmounts,
)
    requires
        crate::model::available_is_exact(account, available),
        requested.spec_le(available),
    ensures capacity_fits(account, requested),
{
}

pub(crate) proof fn request_capacity_from_available(
    account: crate::state::BudgetAccount,
    request: crate::BudgetRequest,
    total: BudgetAmounts,
    available: BudgetAmounts,
)
    requires
        crate::model::available_is_exact(account, available),
        BudgetAmounts::spec_sum(total, request.spec_consume_now(), request.spec_reserve()),
        total.spec_le(available),
    ensures request_capacity_fits(account, request),
{
}

pub(crate) open spec fn accepted_command_guard(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    kind: BudgetReceiptKind,
) -> bool {
    crate::model::ledger_well_formed(ledger)
        && match command {
            BudgetCommand::AllocateChild(request) => allocate_guard(ledger, request, kind),
            BudgetCommand::Begin(request) => begin_guard(ledger, request, kind),
            BudgetCommand::Activate(activation) => operations::activation_guard(ledger, activation, kind),
            BudgetCommand::ObserveUsage(observation) => {
                observation_guard(ledger, observation, kind)
            }
            BudgetCommand::SettleExact(reference) => full_finalization_guard(
                ledger,
                reference,
                ReservationPhase::SettledExact,
                kind,
            ),
            BudgetCommand::CancelHeld(reference) => cancellation_guard(ledger, reference, kind),
            BudgetCommand::FinalizeAmbiguous(finalization) => full_finalization_guard(
                ledger,
                finalization.spec_reference(),
                ReservationPhase::SettledAmbiguous,
                kind,
            ),
            BudgetCommand::Seal(budget_id) => outcomes::lifecycle_guard(ledger, budget_id, kind, false),
            BudgetCommand::Close(budget_id) => outcomes::lifecycle_guard(ledger, budget_id, kind, true),
        }
}

} // verus!
