//! Allocation, begin, and activation admissibility.

#[cfg(verus_only)]
use crate::{BudgetLedger, BudgetReceiptKind, ReservationPhase};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn allocate_guard(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    kind: BudgetReceiptKind,
) -> bool {
    match kind {
        BudgetReceiptKind::Idempotent => exists |index: int| #![auto]
            super::account_at(ledger, request.spec_child_id(), index)
                && crate::identity_model::parent_matches(
                    ledger.accounts[index].parent_id,
                    request.spec_parent_id(),
                )
                && crate::identity_model::revisions_equal(
                    ledger.accounts[index].revision,
                    request.spec_revision(),
                )
                && ledger.accounts[index].limits.spec_amounts().spec_equal(
                    request.spec_limits().spec_amounts(),
                ),
        BudgetReceiptKind::Applied => {
            !(exists |index: int| #![auto]
                super::account_at(ledger, request.spec_child_id(), index))
                && super::lineage_is_open(ledger, request.spec_parent_id())
                && (exists |parent: int| #![auto]
                    super::account_at(ledger, request.spec_parent_id(), parent)
                        && crate::identity_model::revisions_equal(
                            ledger.accounts[parent].revision,
                            request.spec_revision(),
                        )
                        && super::capacity_fits(
                            ledger.accounts[parent],
                            request.spec_limits().spec_amounts(),
                        ))
        }
        BudgetReceiptKind::OverrunFaulted => false,
    }
}

pub(crate) open spec fn begin_guard(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    kind: BudgetReceiptKind,
) -> bool {
    match kind {
        BudgetReceiptKind::Idempotent => exists |index: int| #![auto]
            super::reservation_at(ledger, request.spec_reservation_id(), index)
                && crate::refinement_model::requests_equal(
                    ledger.reservations[index].request,
                    request,
                ),
        BudgetReceiptKind::Applied => {
            !(exists |index: int| #![auto]
                super::reservation_at(ledger, request.spec_reservation_id(), index))
                && (!request.spec_consume_now().spec_is_zero()
                    || !request.spec_reserve().spec_is_zero())
                && super::lineage_is_open(ledger, request.spec_budget_id())
                && crate::invariant::prior_history_resolved(
                    ledger,
                    request,
                    ledger.reservations@.len() as int,
                )
                && crate::invariant::attempt_charge_valid(
                    request,
                    crate::invariant::prior_exact_request(
                        ledger,
                        request,
                        ledger.reservations@.len() as int,
                    ),
                )
                && (exists |account: int| #![auto]
                    super::account_at(ledger, request.spec_budget_id(), account)
                        && crate::identity_model::revisions_equal(
                            ledger.accounts[account].revision,
                            request.spec_revision(),
                        )
                        && super::request_capacity_fits(ledger.accounts[account], request))
        }
        BudgetReceiptKind::OverrunFaulted => false,
    }
}

pub(crate) open spec fn activation_guard(
    ledger: &BudgetLedger,
    activation: crate::Activation,
    kind: BudgetReceiptKind,
) -> bool {
    exists |index: int| #![auto]
        super::reservation_at(ledger, activation.spec_reservation_id(), index)
            && super::activation_binding(ledger.reservations[index], activation)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[index].phase == ReservationPhase::Held
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[index].phase == ReservationPhase::Active
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[index].activation_evidence,
                            Some(activation.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) proof fn allocate_guard_from_runtime(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    kind: BudgetReceiptKind,
    witness: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        match kind {
            BudgetReceiptKind::Idempotent => {
                super::account_at(ledger, request.spec_child_id(), witness)
                    && crate::identity_model::parent_matches(
                        ledger.accounts[witness].parent_id,
                        request.spec_parent_id(),
                    )
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[witness].revision,
                        request.spec_revision(),
                    )
                    && ledger.accounts[witness].limits.spec_amounts().spec_equal(
                        request.spec_limits().spec_amounts(),
                    )
            }
            BudgetReceiptKind::Applied => {
                (forall |index: int| #![auto]
                    0 <= index < ledger.accounts@.len()
                        ==> !crate::identity_model::budget_ids_equal(
                            ledger.accounts[index].id,
                            request.spec_child_id(),
                        ))
                    && super::lineage_is_open(ledger, request.spec_parent_id())
                    && super::account_at(ledger, request.spec_parent_id(), witness)
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[witness].revision,
                        request.spec_revision(),
                    )
                    && super::capacity_fits(
                        ledger.accounts[witness],
                        request.spec_limits().spec_amounts(),
                    )
            }
            BudgetReceiptKind::OverrunFaulted => false,
        },
    ensures super::accepted_command_guard(
        ledger,
        crate::BudgetCommand::AllocateChild(request),
        kind,
    ),
{
    match kind {
        BudgetReceiptKind::Idempotent => {
            assert(exists |index: int| #![auto]
                super::account_at(ledger, request.spec_child_id(), index)
                    && crate::identity_model::parent_matches(
                        ledger.accounts[index].parent_id,
                        request.spec_parent_id(),
                    )
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[index].revision,
                        request.spec_revision(),
                    )
                    && ledger.accounts[index].limits.spec_amounts().spec_equal(
                        request.spec_limits().spec_amounts(),
                    ));
        }
        BudgetReceiptKind::Applied => {
            assert(!(exists |index: int| #![auto]
                super::account_at(ledger, request.spec_child_id(), index)));
            assert(exists |parent: int| #![auto]
                super::account_at(ledger, request.spec_parent_id(), parent)
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[parent].revision,
                        request.spec_revision(),
                    )
                    && super::capacity_fits(
                        ledger.accounts[parent],
                        request.spec_limits().spec_amounts(),
                    ));
        }
        BudgetReceiptKind::OverrunFaulted => {}
    }
}

pub(crate) proof fn begin_guard_from_runtime(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    kind: BudgetReceiptKind,
    witness: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        match kind {
            BudgetReceiptKind::Idempotent => {
                super::reservation_at(ledger, request.spec_reservation_id(), witness)
                    && crate::refinement_model::requests_equal(
                        ledger.reservations[witness].request,
                        request,
                    )
            }
            BudgetReceiptKind::Applied => {
                (forall |index: int| #![auto]
                    0 <= index < ledger.reservations@.len()
                        ==> !crate::identity_model::reservation_ids_equal(
                            ledger.reservations[index].request.spec_reservation_id(),
                            request.spec_reservation_id(),
                        ))
                    && (!request.spec_consume_now().spec_is_zero()
                        || !request.spec_reserve().spec_is_zero())
                    && super::lineage_is_open(ledger, request.spec_budget_id())
                    && crate::invariant::prior_history_resolved(
                        ledger,
                        request,
                        ledger.reservations@.len() as int,
                    )
                    && crate::invariant::attempt_charge_valid(
                        request,
                        crate::invariant::prior_exact_request(
                            ledger,
                            request,
                            ledger.reservations@.len() as int,
                        ),
                    )
                    && super::account_at(ledger, request.spec_budget_id(), witness)
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[witness].revision,
                        request.spec_revision(),
                    )
                    && super::request_capacity_fits(ledger.accounts[witness], request)
            }
            BudgetReceiptKind::OverrunFaulted => false,
        },
    ensures super::accepted_command_guard(
        ledger,
        crate::BudgetCommand::Begin(request),
        kind,
    ),
{
    match kind {
        BudgetReceiptKind::Idempotent => {
            assert(exists |index: int| #![auto]
                super::reservation_at(ledger, request.spec_reservation_id(), index)
                    && crate::refinement_model::requests_equal(
                        ledger.reservations[index].request,
                        request,
                    ));
        }
        BudgetReceiptKind::Applied => {
            assert(!(exists |index: int| #![auto]
                super::reservation_at(ledger, request.spec_reservation_id(), index)));
            assert(exists |account: int| #![auto]
                super::account_at(ledger, request.spec_budget_id(), account)
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[account].revision,
                        request.spec_revision(),
                    )
                    && super::request_capacity_fits(ledger.accounts[account], request));
        }
        BudgetReceiptKind::OverrunFaulted => {}
    }
}

pub(crate) proof fn activation_guard_from_runtime(
    ledger: &BudgetLedger,
    activation: crate::Activation,
    kind: BudgetReceiptKind,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        super::reservation_at(ledger, activation.spec_reservation_id(), index),
        super::activation_binding(ledger.reservations[index], activation),
        match kind {
            BudgetReceiptKind::Applied => {
                ledger.reservations[index].phase == ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                ledger.reservations[index].phase == ReservationPhase::Active
                    && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].activation_evidence,
                        Some(activation.spec_evidence_digest()),
                    )
            }
            BudgetReceiptKind::OverrunFaulted => false,
        },
    ensures super::accepted_command_guard(
        ledger,
        crate::BudgetCommand::Activate(activation),
        kind,
    ),
{
    assert(exists |witness: int| #![auto]
        super::reservation_at(ledger, activation.spec_reservation_id(), witness)
            && super::activation_binding(ledger.reservations[witness], activation)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[witness].phase == ReservationPhase::Held
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[witness].phase == ReservationPhase::Active
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[witness].activation_evidence,
                            Some(activation.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            });
}

} // verus!
