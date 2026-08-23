//! Stable crate-level projections over private command guards and rejection relations.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetError, BudgetLedger};
use vstd::prelude::*;

verus! {

/// The only admitted initial states are validated single-root ledgers.
pub(crate) closed spec fn initial_state(ledger: &BudgetLedger) -> bool {
    ledger.spec_is_initial()
}

pub(crate) open spec fn exact_input_view(
    concrete: &BudgetLedger,
    logical: &BudgetLedger,
) -> bool {
    crate::identity_model::budget_ids_equal(concrete.root_id, logical.root_id)
        && concrete.accounts@ == logical.accounts@
        && concrete.reservations@ == logical.reservations@
}

pub(crate) proof fn exact_input_view_preserves_well_formed(
    source: &BudgetLedger,
    duplicate: &BudgetLedger,
)
    requires
        crate::model::ledger_well_formed(source),
        exact_input_view(source, duplicate),
    ensures crate::model::ledger_well_formed(duplicate),
{
    super::equivalence::well_formed_transfers(source, duplicate);
}

pub(crate) open spec fn accepted_guard(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    kind: crate::BudgetReceiptKind,
) -> bool {
    super::guards::accepted_command_guard(ledger, command, kind)
}

pub(crate) open spec fn full_finalization_guard_exact(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    phase: crate::ReservationPhase,
    kind: crate::BudgetReceiptKind,
) -> bool {
    super::guards::full_finalization_guard(ledger, reference, phase, kind)
}

pub(crate) open spec fn open_lineage_guard(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::guards::lineage_is_open(ledger, budget_id)
}

pub(crate) open spec fn open_parent_chain_guard(ledger: &BudgetLedger, index: int) -> bool {
    super::guards::open_parent_chain(ledger, index)
}

pub(crate) open spec fn account_at_guard(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    index: int,
) -> bool {
    super::guards::account_at(ledger, budget_id, index)
}

pub(crate) open spec fn reservation_at_guard(
    ledger: &BudgetLedger,
    reservation_id: peritus_types::BudgetReservationId,
    index: int,
) -> bool {
    super::guards::reservation_at(ledger, reservation_id, index)
}

pub(crate) open spec fn activation_binding_guard(
    record: crate::state::ReservationRecord,
    activation: crate::Activation,
) -> bool {
    super::guards::activation_binding(record, activation)
}

pub(crate) open spec fn capacity_guard(
    account: crate::state::BudgetAccount,
    requested: crate::BudgetAmounts,
) -> bool {
    super::guards::capacity_fits(account, requested)
}

pub(crate) open spec fn request_capacity_guard(
    account: crate::state::BudgetAccount,
    request: crate::BudgetRequest,
) -> bool {
    super::guards::request_capacity_fits(account, request)
}

pub(crate) open spec fn rejection_cause(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
) -> bool {
    super::rejections::rejection_cause(ledger, command, error)
}

pub(crate) open spec fn budget_error_matches(
    error: BudgetError,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::rejections::budget_error_matches(error, budget_id)
}

pub(crate) open spec fn reservation_error_matches(
    error: BudgetError,
    reservation_id: peritus_types::BudgetReservationId,
) -> bool {
    super::rejections::reservation_error_matches(error, reservation_id)
}

pub(crate) open spec fn exact_budget_error(
    error: BudgetError,
    kind: crate::BudgetErrorKind,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::rejections::exact_budget_error(error, kind, budget_id)
}

pub(crate) open spec fn exact_reservation_error(
    error: BudgetError,
    kind: crate::BudgetErrorKind,
    reservation_id: peritus_types::BudgetReservationId,
) -> bool {
    super::rejections::exact_reservation_error(error, kind, reservation_id)
}

pub(crate) open spec fn infrastructure_error(error: BudgetError) -> bool {
    super::rejections::infrastructure_error(error)
}

pub(crate) open spec fn lineage_rejection(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::lineage_rejection(ledger, budget_id, error)
}

pub(crate) open spec fn first_non_open_account(
    ledger: &BudgetLedger,
    index: int,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::first_non_open_account(ledger, index, error)
}

pub(crate) open spec fn retry_history_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    end: int,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::retry_history_rejection(ledger, request, end, error)
}

pub(crate) open spec fn begin_after_identity_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::begin_after_identity_rejection(ledger, request, error)
}

pub(crate) open spec fn begin_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::begin_rejection(ledger, request, error)
}

pub(crate) open spec fn allocation_rejection(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    error: BudgetError,
) -> bool {
    super::rejections::allocation::allocation_rejection(ledger, request, error)
}

pub(crate) open spec fn reference_binding_guard(
    record: crate::state::ReservationRecord,
    reference: crate::ReservationReference,
) -> bool {
    super::guards::reference_binding(record, reference)
}

pub(crate) open spec fn observation_binding_guard(
    record: crate::state::ReservationRecord,
    observation: crate::UsageObservation,
) -> bool {
    super::guards::observation_binding(record, observation)
}

pub(crate) open spec fn terminal_observation_guard(
    record: crate::state::ReservationRecord,
    observation: crate::UsageObservation,
) -> bool {
    super::guards::terminal_observation_matches(record, observation)
}

} // verus!
