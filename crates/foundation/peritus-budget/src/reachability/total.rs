//! Total accepted/rejected outcomes and finite accepted-state reachability.

#[cfg(verus_only)]
use super::BudgetStepOutcome;
#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetError, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn ledgers_exactly_equal(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    super::commands::ledgers_exactly_equal(before, after)
}

pub(crate) proof fn ledger_exact_reflexive(ledger: &BudgetLedger)
    ensures ledgers_exactly_equal(ledger, ledger),
{
    super::commands::ledger_equality_reflexive(ledger);
}

pub(crate) proof fn allocate_idempotent_refines(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    receipt: BudgetReceipt,
)
    requires
        super::guards::accepted_command_guard(
            ledger,
            BudgetCommand::AllocateChild(request),
            receipt.spec_kind(),
        ),
        receipt.spec_operation() == crate::BudgetOperation::AllocateChild,
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        crate::identity_model::budget_ids_equal(
            receipt.spec_budget_id(), request.spec_child_id(),
        ),
        receipt.spec_reservation_id().is_none(),
        receipt.spec_charged().spec_is_zero(),
        receipt.spec_released().spec_is_zero(),
        receipt.spec_reported().is_none(),
        receipt.spec_evidence_digest().is_none(),
    ensures super::candidate_step(
        ledger, BudgetCommand::AllocateChild(request), ledger, receipt,
    ),
{
    super::commands::ledger_equality_reflexive(ledger);
    super::raw_step_is_accepted(
        ledger, BudgetCommand::AllocateChild(request), ledger, receipt,
    );
}

pub(crate) open spec fn child_allocation_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: crate::ChildBudgetRequest,
) -> bool {
    super::allocation::child_allocation_effect(before, after, request)
}

pub(crate) proof fn allocate_applied_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    receipt: BudgetReceipt,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::AllocateChild(request),
            receipt.spec_kind(),
        ),
        child_allocation_exact(before, after, request),
        receipt.spec_operation() == crate::BudgetOperation::AllocateChild,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied,
        crate::identity_model::budget_ids_equal(
            receipt.spec_budget_id(), request.spec_child_id(),
        ),
        receipt.spec_reservation_id().is_none(),
        receipt.spec_charged().spec_is_zero(),
        receipt.spec_released().spec_is_zero(),
        receipt.spec_reported().is_none(),
        receipt.spec_evidence_digest().is_none(),
    ensures super::candidate_step(
        before, BudgetCommand::AllocateChild(request), after, receipt,
    ),
{
    super::raw_step_is_accepted(
        before, BudgetCommand::AllocateChild(request), after, receipt,
    );
}

/// Total closed relation for accepted and rejected outcomes.
pub(crate) closed spec fn budget_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    outcome: BudgetStepOutcome,
) -> bool {
    match outcome {
        BudgetStepOutcome::Accepted(after, receipt) => {
            super::accepted_step(before, command, &after, receipt)
                && crate::model::ledger_well_formed(&after)
        }
        BudgetStepOutcome::Rejected(preserved, error) => {
            super::rejected_step(before, command, error, &preserved)
        }
    }
}

pub(crate) proof fn accepted_well_formed_is_budget_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
)
    requires
        super::accepted_step(before, command, after, receipt),
        crate::model::ledger_well_formed(after),
    ensures budget_step(
        before, command, BudgetStepOutcome::Accepted(*after, receipt),
    ),
{
}

pub(crate) proof fn rejected_is_budget_step(
    before: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
)
    requires super::rejected_step(before, command, error, before),
    ensures budget_step(
        before, command, BudgetStepOutcome::Rejected(*before, error),
    ),
{
}

/// Recursive finite reachability over the closed accepted-step relation.
pub(crate) open spec fn reachable_after(
    initial: &BudgetLedger,
    commands: Seq<BudgetCommand>,
    states: Seq<BudgetLedger>,
    receipts: Seq<BudgetReceipt>,
) -> bool
    decreases commands.len(),
{
    if commands.len() == 0 {
        states.len() == 1
            && receipts.len() == 0
            && super::commands::ledgers_exactly_equal(initial, &states[0])
    } else {
        states.len() == commands.len() + 1
            && receipts.len() == commands.len()
            && super::initial_state(initial)
            && super::accepted_step(
                &states[commands.len() - 1],
                commands[commands.len() - 1],
                &states[commands.len() as int],
                receipts[commands.len() - 1],
            )
            && reachable_after(
                initial,
                commands.drop_last(),
                states.drop_last(),
                receipts.drop_last(),
            )
    }
}

} // verus!
