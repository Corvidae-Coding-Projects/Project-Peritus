//! Exact activation and account lifecycle refinement lemmas.

#[cfg(verus_only)]
use crate::{BudgetCommand, BudgetLedger, BudgetReceipt};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn activation_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    activation: crate::Activation,
) -> bool {
    super::commands::accounts_exactly_equal(before, after)
        && super::reservations::activation_effect(
            before,
            after,
            activation.spec_reservation_id(),
            activation.spec_evidence_digest(),
        )
}

pub(crate) open spec fn activation_receipt_exact(
    receipt: BudgetReceipt,
    activation: crate::Activation,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::commands::receipt_identity(
        receipt,
        crate::BudgetOperation::Activate,
        budget_id,
        Some(activation.spec_reservation_id()),
    )
        && receipt.spec_charged().spec_is_zero()
        && receipt.spec_released().spec_is_zero()
        && receipt.spec_reported().is_none()
        && crate::invariant::optional_digests_equal(
            receipt.spec_evidence_digest(),
            Some(activation.spec_evidence_digest()),
        )
}

pub(crate) proof fn activation_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    activation: crate::Activation,
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::Activate(activation),
            receipt.spec_kind(),
        ),
        super::reservation_bound_to_budget(
            before,
            activation.spec_reservation_id(),
            budget_id,
        ),
        activation_receipt_exact(receipt, activation, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> activation_exact(before, after, activation),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> super::ledgers_exactly_equal(before, after),
    ensures
        super::candidate_step(
            before,
            BudgetCommand::Activate(activation),
            after,
            receipt,
        ),
{
    super::raw_step_is_accepted(
        before,
        BudgetCommand::Activate(activation),
        after,
        receipt,
    );
}

pub(crate) open spec fn seal_effect_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::accounts::account_phase_effect(
        before,
        after,
        budget_id,
        crate::BudgetAccountPhase::Draining,
    ) && super::commands::reservations_exactly_equal(before, after)
}

pub(crate) open spec fn seal_receipt_exact(
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::commands::receipt_identity(receipt, crate::BudgetOperation::Seal, budget_id, None)
        && receipt.spec_charged().spec_is_zero()
        && receipt.spec_released().spec_is_zero()
        && super::commands::receipt_has_no_observation(receipt)
}

pub(crate) proof fn seal_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    receipt: BudgetReceipt,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::Seal(budget_id),
            receipt.spec_kind(),
        ),
        seal_receipt_exact(receipt, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> seal_effect_exact(before, after, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> super::ledgers_exactly_equal(before, after),
    ensures
        super::candidate_step(before, BudgetCommand::Seal(budget_id), after, receipt),
{
    super::raw_step_is_accepted(before, BudgetCommand::Seal(budget_id), after, receipt);
}

pub(crate) open spec fn close_effect_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    receipt: BudgetReceipt,
) -> bool {
    super::allocation::close_account_effect(
        before,
        after,
        budget_id,
        receipt.spec_released(),
    )
}

pub(crate) open spec fn immutable_account_fields_equal(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    super::accounts::immutable_account_fields_equal(before, after)
}

pub(crate) open spec fn close_receipt_exact(
    receipt: BudgetReceipt,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::commands::receipt_identity(receipt, crate::BudgetOperation::Close, budget_id, None)
        && receipt.spec_charged().spec_is_zero()
        && super::commands::receipt_has_no_observation(receipt)
}

pub(crate) proof fn close_refines(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    receipt: BudgetReceipt,
)
    requires
        super::guards::accepted_command_guard(
            before,
            BudgetCommand::Close(budget_id),
            receipt.spec_kind(),
        ),
        close_receipt_exact(receipt, budget_id),
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            || receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent,
        receipt.spec_kind() == crate::BudgetReceiptKind::Applied
            ==> close_effect_exact(before, after, budget_id, receipt),
        receipt.spec_kind() == crate::BudgetReceiptKind::Idempotent
            ==> receipt.spec_released().spec_is_zero()
                && super::ledgers_exactly_equal(before, after),
    ensures
        super::candidate_step(before, BudgetCommand::Close(budget_id), after, receipt),
{
    super::raw_step_is_accepted(before, BudgetCommand::Close(budget_id), after, receipt);
}

} // verus!
