//! Closed reachability and total command/outcome relation for the budget reducer.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetError, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

mod accounts;
mod account_updates;
mod allocation;
mod cancellation_proofs;
mod commands;
mod executable_bridge;
mod equivalence;
mod guards;
mod functionality;
mod initial_proof;
mod lifecycle_proofs;
mod lifecycle_steps;
mod public_model;
mod reducer_proofs;
mod rejections;
mod reservations;
mod total;

#[cfg(verus_only)]
pub(crate) use reducer_proofs::{
    ambiguous_finalization_refines, begin_receipt_exact, begin_record_exact, begin_refines,
    full_finalization_exact,
    full_finalization_receipt_exact, full_finalization_record_exact,
    full_finalization_refines, observation_receipt_exact,
    observation_record_exact, observation_refines, overrun_active_effect_exact,
    overrun_terminal_effect_exact, reservation_bound_to_budget,
    settle_exact_refines,
};
#[cfg(verus_only)]
pub(crate) use commands::observation_overrun_effect;

#[cfg(verus_only)]
pub(crate) use cancellation_proofs::{
    cancellation_receipt_exact, cancellation_record_exact, cancellation_refines,
};

#[cfg(verus_only)]
pub(crate) use lifecycle_proofs::{
    activation_exact, activation_receipt_exact, activation_refines, close_effect_exact,
    close_receipt_exact, close_refines, immutable_account_fields_equal, seal_effect_exact,
    seal_receipt_exact, seal_refines,
};

#[cfg(verus_only)]
pub(crate) use reservations::activation_effect_from_update;
#[cfg(verus_only)]
pub(crate) use accounts::account_phase_effect_from_record;

#[cfg(verus_only)]
pub(crate) use initial_proof::single_root_is_well_formed;
#[cfg(verus_only)]
pub(crate) use functionality::candidate_step_is_functional;

#[cfg(verus_only)]
pub(crate) use public_model::{
    accepted_guard, account_at_guard, activation_binding_guard, allocation_rejection,
    begin_after_identity_rejection, begin_rejection, budget_error_matches, capacity_guard,
    exact_budget_error, exact_input_view, exact_input_view_preserves_well_formed,
    exact_reservation_error, first_non_open_account, full_finalization_guard_exact,
    infrastructure_error, initial_state, lineage_rejection, observation_binding_guard, open_lineage_guard,
    open_parent_chain_guard, reference_binding_guard, rejection_cause, request_capacity_guard,
    reservation_at_guard, reservation_error_matches, retry_history_rejection,
    terminal_observation_guard,
};

#[cfg(verus_only)]
pub(crate) use total::{
    accepted_well_formed_is_budget_step, allocate_applied_refines,
    allocate_idempotent_refines, budget_step, child_allocation_exact, ledger_exact_reflexive,
    ledgers_exactly_equal, reachable_after, rejected_is_budget_step,
};

#[cfg(verus_only)]
pub(crate) use executable_bridge::{
    begin_accounting_exact, charged_account_exact, faulted_account_exact,
    lineage_charge_exact, lineage_charge_fuel_exact, lineage_charge_preserves_account_id,
    lineage_fault_exact,
    lineage_fault_fuel_exact, observation_accounting_exact, operation_release_exact,
    operation_release_preserves_account_identity,
    operation_reserve_exact, overrun_accounting_exact, released_account_exact,
    reservation_accounting_exact, reserved_account_exact,
};

#[cfg(verus_only)]
pub(crate) use guards::{
    activation_guard_from_runtime, allocate_guard_from_runtime, begin_guard_from_runtime,
    capacity_from_available, absent_account_has_no_lineage,
    cancellation_guard_from_runtime, finalization_guard_from_runtime,
    lifecycle_guard_from_runtime, observation_guard_from_runtime,
    account_without_chain_has_no_lineage, non_open_head_has_no_chain, open_chain_from_parent,
    open_chain_implies_parent_chain, open_chain_root, request_capacity_from_available,
};

verus! {

/// Closed result vocabulary for one mathematical budget step.
#[cfg(verus_only)]
pub(crate) enum BudgetStepOutcome {
    /// The command produced one exact successor and receipt.
    Accepted(BudgetLedger, BudgetReceipt),
    /// The command produced no successor; the modeled state remains the input ledger.
    Rejected(BudgetLedger, BudgetError),
}

pub(crate) open spec fn raw_accepted_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    guards::accepted_command_guard(before, command, receipt.spec_kind())
        && match command {
        BudgetCommand::AllocateChild(request) => {
            commands::allocate_child_step(before, request, after, receipt)
        }
        BudgetCommand::Begin(request) => commands::begin_step(before, request, after, receipt),
        BudgetCommand::Activate(activation) => {
            commands::activate_step(before, activation, after, receipt)
        }
        BudgetCommand::ObserveUsage(observation) => {
            commands::observation_step(before, observation, after, receipt)
        }
        BudgetCommand::SettleExact(_)
        | BudgetCommand::CancelHeld(_)
        | BudgetCommand::FinalizeAmbiguous(_)
        | BudgetCommand::Seal(_)
        | BudgetCommand::Close(_) => {
            lifecycle_steps::lifecycle_step(before, command, after, receipt)
        }
    }
}

/// Exact candidate successor relation produced by the command-local reducer.
pub(crate) closed spec fn candidate_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    exists |logical_before: BudgetLedger| #![auto]
        exact_input_view(before, &logical_before)
            && raw_accepted_step(&logical_before, command, after, receipt)
}

/// Exact accepted successor relation after concrete invariant and refinement validation.
pub(crate) closed spec fn accepted_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
) -> bool {
    candidate_step(before, command, after, receipt)
        && crate::model::ledger_well_formed(after)
        && complete_refinement(before, after)
}

pub(crate) proof fn accepted_step_is_valid_and_complete(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
)
    requires
        crate::model::ledger_well_formed(before),
        accepted_step(before, command, after, receipt),
    ensures
        crate::model::ledger_well_formed(after),
        complete_refinement(before, after),
{
}

pub(crate) proof fn raw_step_is_accepted(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
)
    requires raw_accepted_step(before, command, after, receipt),
    ensures candidate_step(before, command, after, receipt),
{
    assert(crate::identity_model::budget_ids_equal(before.root_id, before.root_id));
    assert(exact_input_view(before, before));
}

pub(crate) proof fn duplicate_input_refines_accepted_step(
    original: &BudgetLedger,
    duplicate: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
)
    requires
        duplicate.root_id == original.root_id,
        crate::identity_model::budget_ids_equal(duplicate.root_id, original.root_id),
        duplicate.accounts@ == original.accounts@,
        duplicate.reservations@ == original.reservations@,
        candidate_step(duplicate, command, after, receipt),
    ensures
        candidate_step(original, command, after, receipt),
{
    let logical_before = choose |logical_before: BudgetLedger| #![auto]
        exact_input_view(duplicate, &logical_before)
            && raw_accepted_step(&logical_before, command, after, receipt);
    assert(exact_input_view(duplicate, &logical_before));
    assert(raw_accepted_step(&logical_before, command, after, receipt));
    assert(crate::identity_model::budget_ids_equal(
        original.root_id,
        logical_before.root_id,
    ));
    assert(exact_input_view(original, &logical_before));
}

pub(crate) proof fn duplicate_input_refines_rejection_cause(
    original: &BudgetLedger,
    duplicate: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
)
    requires
        duplicate.root_id == original.root_id,
        crate::identity_model::budget_ids_equal(duplicate.root_id, original.root_id),
        duplicate.accounts@ == original.accounts@,
        duplicate.reservations@ == original.reservations@,
        rejections::rejection_cause(duplicate, command, error),
    ensures semantic_rejection_cause(original, command, error),
{
    assert(exact_input_view(original, duplicate));
}

pub(crate) open spec fn semantic_rejection_cause(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
) -> bool {
    exists |logical_before: BudgetLedger| #![auto]
        exact_input_view(ledger, &logical_before)
            && rejections::rejection_cause(&logical_before, command, error)
}

pub(crate) open spec fn complete_refinement(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    crate::model::ledger_consumption_monotonic(before, after)
        && crate::model::ledger_high_water_monotonic(before, after)
        && crate::refinement_model::ledger_identity_stable(before, after)
        && crate::refinement_model::ancestor_consumption_propagates(before, after)
}

pub(crate) open spec fn exact_corrupt_root(
    ledger: &BudgetLedger,
    error: BudgetError,
) -> bool {
    rejections::exact_budget_error(error, crate::BudgetErrorKind::CorruptState, ledger.root_id)
}

pub(crate) open spec fn transition_rejection_cause(
    before: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
) -> bool {
    if !crate::model::ledger_well_formed(before) {
        exact_corrupt_root(before, error)
    } else {
        semantic_rejection_cause(before, command, error)
            || ((exists |intermediate: BudgetLedger, receipt: BudgetReceipt| #![auto]
                    candidate_step(before, command, &intermediate, receipt)
                        && !crate::model::ledger_well_formed(&intermediate))
                && exact_corrupt_root(before, error))
            || ((exists |intermediate: BudgetLedger, receipt: BudgetReceipt| #![auto]
                    candidate_step(before, command, &intermediate, receipt)
                        && crate::model::ledger_well_formed(&intermediate)
                        && !complete_refinement(before, &intermediate))
                && exact_corrupt_root(before, error))
    }
}

/// A rejected pure command has no candidate successor and preserves the modeled ledger exactly.
pub(crate) closed spec fn rejected_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
    preserved: &BudgetLedger,
) -> bool {
    transition_rejection_cause(before, command, error)
        && commands::ledgers_exactly_equal(before, preserved)
}

pub(crate) proof fn rejected_state_reflexive(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
)
    requires transition_rejection_cause(ledger, command, error),
    ensures rejected_step(ledger, command, error, ledger),
{
    commands::ledger_equality_reflexive(ledger);
}

pub(crate) proof fn malformed_input_rejects(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
)
    requires
        !crate::model::ledger_well_formed(ledger),
        exact_corrupt_root(ledger, error),
    ensures rejected_step(ledger, command, error, ledger),
{
    commands::ledger_equality_reflexive(ledger);
}

pub(crate) proof fn validated_candidate_is_accepted(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
)
    requires
        candidate_step(before, command, after, receipt),
        crate::model::ledger_well_formed(after),
        complete_refinement(before, after),
    ensures accepted_step(before, command, after, receipt),
{
}

pub(crate) proof fn semantic_error_rejects(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
)
    requires semantic_rejection_cause(ledger, command, error),
    ensures rejected_step(ledger, command, error, ledger),
{
    let logical_before = choose |logical_before: BudgetLedger| #![auto]
        exact_input_view(ledger, &logical_before)
            && rejections::rejection_cause(&logical_before, command, error);
    assert(crate::model::ledger_well_formed(&logical_before));
    assert(exact_input_view(&logical_before, ledger)) by {
        assert(ledger.accounts@ == logical_before.accounts@);
        assert(ledger.reservations@ == logical_before.reservations@);
    }
    exact_input_view_preserves_well_formed(&logical_before, ledger);
    commands::ledger_equality_reflexive(ledger);
}

pub(crate) proof fn invalid_successor_rejects(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    intermediate: &BudgetLedger,
    receipt: BudgetReceipt,
    error: BudgetError,
)
    requires
        crate::model::ledger_well_formed(ledger),
        candidate_step(ledger, command, intermediate, receipt),
        !crate::model::ledger_well_formed(intermediate),
        exact_corrupt_root(ledger, error),
    ensures rejected_step(ledger, command, error, ledger),
{
    commands::ledger_equality_reflexive(ledger);
    assert(exists |witness: BudgetLedger, witness_receipt: BudgetReceipt| #![auto]
        witness == *intermediate
            && witness_receipt == receipt
            && candidate_step(ledger, command, &witness, witness_receipt)
            && !crate::model::ledger_well_formed(&witness));
}

pub(crate) proof fn invalid_refinement_rejects(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    intermediate: &BudgetLedger,
    receipt: BudgetReceipt,
    error: BudgetError,
)
    requires
        crate::model::ledger_well_formed(ledger),
        candidate_step(ledger, command, intermediate, receipt),
        crate::model::ledger_well_formed(intermediate),
        !complete_refinement(ledger, intermediate),
        exact_corrupt_root(ledger, error),
    ensures rejected_step(ledger, command, error, ledger),
{
    commands::ledger_equality_reflexive(ledger);
    assert(exists |witness: BudgetLedger, witness_receipt: BudgetReceipt| #![auto]
        witness == *intermediate
            && witness_receipt == receipt
            && candidate_step(ledger, command, &witness, witness_receipt)
            && crate::model::ledger_well_formed(&witness)
            && !complete_refinement(ledger, &witness));
}

} // verus!
