//! Witness projections for exact active-overrun accounting.

#[cfg(verus_only)]
use crate::{BudgetLedger, BudgetReceipt, ReservationPhase, UsageObservation};
use vstd::prelude::*;

verus! {

pub(super) open spec fn overrun_release_witness(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget: peritus_types::BudgetId,
    receipt: BudgetReceipt,
    released: BudgetLedger,
) -> bool {
    crate::reachability::commands::observation_overrun_release_effect(
        before, after, receipt, budget, released,
    )
}

pub(super) open spec fn overrun_charged_witness(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget: peritus_types::BudgetId,
    receipt: BudgetReceipt,
    released: &BudgetLedger,
    charged_state: BudgetLedger,
) -> bool {
    crate::reachability::commands::observation_overrun_charged_effect(
        before, after, receipt, budget, released, charged_state,
    )
}

pub(super) proof fn overrun_effect_has_release(
    before: &BudgetLedger,
    observation: UsageObservation,
    after: &BudgetLedger,
    receipt: BudgetReceipt,
    budget: peritus_types::BudgetId,
    index: int,
)
    requires
        crate::reachability::commands::observation_overrun_effect(
            before, observation, after, receipt, budget, index,
        ),
        before.reservations[index].phase == ReservationPhase::Active,
    ensures exists |released: BudgetLedger| #![auto]
        overrun_release_witness(before, after, budget, receipt, released),
{
    reveal(crate::reachability::commands::observation_overrun_effect);
    reveal(crate::reachability::commands::observation_overrun_release_effect);
    reveal(overrun_release_witness);
    let released = choose |released: BudgetLedger| #![auto]
        crate::reachability::commands::observation_overrun_release_effect(
            before, after, receipt, budget, released,
        );
    assert(overrun_release_witness(before, after, budget, receipt, released));
    assert(exists |witness: BudgetLedger| #![auto]
        overrun_release_witness(before, after, budget, receipt, witness));
}

pub(super) proof fn overrun_release_has_charged(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget: peritus_types::BudgetId,
    receipt: BudgetReceipt,
    released: &BudgetLedger,
)
    requires overrun_release_witness(before, after, budget, receipt, *released),
    ensures exists |charged_state: BudgetLedger| #![auto]
        overrun_charged_witness(
            before, after, budget, receipt, released, charged_state,
        ),
{
    reveal(overrun_release_witness);
    reveal(overrun_charged_witness);
    reveal(crate::reachability::commands::observation_overrun_release_effect);
    reveal(crate::reachability::commands::observation_overrun_charged_effect);
    let charged_state = choose |charged_state: BudgetLedger| #![auto]
        crate::reachability::commands::observation_overrun_charged_effect(
            before, after, receipt, budget, released, charged_state,
        );
    assert(overrun_charged_witness(
        before, after, budget, receipt, released, charged_state,
    ));
    assert(exists |witness: BudgetLedger| #![auto]
        overrun_charged_witness(before, after, budget, receipt, released, witness));
}

} // verus!
