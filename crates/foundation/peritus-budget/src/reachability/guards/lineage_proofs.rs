//! Constructor lemmas for the finite open-lineage path witness.

#[cfg(verus_only)]
use crate::{BudgetAccountPhase, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(crate) proof fn open_chain_root(ledger: &BudgetLedger, index: int)
    requires
        0 <= index < ledger.accounts@.len(),
        ledger.accounts[index].phase == BudgetAccountPhase::Open,
        ledger.accounts[index].parent_id.is_none(),
    ensures super::open_parent_chain(ledger, index),
{
    let path = Seq::new(1, |position: int| if position == 0 { index } else { index });
    assert(path.len() == 1);
    assert(path[0] == index);
    assert forall |position: int| #![auto]
        0 <= position < path.len() implies
            0 <= path[position] < ledger.accounts@.len()
                && ledger.accounts[path[position]].phase == BudgetAccountPhase::Open by {
        assert(position == 0);
    }
    assert forall |position: int| #![auto]
        0 <= position && position + 1 < path.len() implies
            0 <= path[position + 1] < path[position]
                && crate::identity_model::parent_matches(
                    ledger.accounts[path[position]].parent_id,
                    ledger.accounts[path[position + 1]].id,
                ) by {
        assert(false);
    }
    assert(path[path.len() - 1] == index);
    assert(super::open_lineage_path(ledger, index, path));
    assert(exists |witness: Seq<int>| #![auto]
        super::open_lineage_path(ledger, index, witness));
}

pub(crate) proof fn open_chain_from_parent(
    ledger: &BudgetLedger,
    index: int,
    parent: int,
)
    requires
        0 <= parent < index < ledger.accounts@.len(),
        ledger.accounts[index].phase == BudgetAccountPhase::Open,
        crate::identity_model::parent_matches(
            ledger.accounts[index].parent_id,
            ledger.accounts[parent].id,
        ),
        super::open_parent_chain(ledger, parent),
    ensures super::open_parent_chain(ledger, index),
{
    let parent_path = choose |path: Seq<int>| #![auto]
        super::open_lineage_path(ledger, parent, path);
    let path = Seq::new(parent_path.len() + 1, |position: int|
        if position == 0 { index } else { parent_path[position - 1] });
    assert(path.len() == parent_path.len() + 1);
    assert(path[0] == index);
    assert forall |position: int| #![auto]
        0 <= position < path.len() implies
            0 <= path[position] < ledger.accounts@.len()
                && ledger.accounts[path[position]].phase == BudgetAccountPhase::Open by {
        if position == 0 {
        } else {
            assert(0 <= position - 1 < parent_path.len());
        }
    }
    assert forall |position: int| #![auto]
        0 <= position && position + 1 < path.len() implies
            0 <= path[position + 1] < path[position]
                && crate::identity_model::parent_matches(
                    ledger.accounts[path[position]].parent_id,
                    ledger.accounts[path[position + 1]].id,
                ) by {
        if position == 0 {
            assert(path[0] == index);
            assert(path[1] == parent_path[0]);
            assert(parent_path[0] == parent);
        } else {
            assert(0 <= position - 1);
            assert(position < parent_path.len());
        }
    }
    assert(path[path.len() - 1] == parent_path[parent_path.len() - 1]);
    assert(super::open_lineage_path(ledger, index, path));
    assert(exists |witness: Seq<int>| #![auto]
        super::open_lineage_path(ledger, index, witness));
}

pub(crate) proof fn non_open_head_has_no_chain(ledger: &BudgetLedger, index: int)
    requires
        0 <= index < ledger.accounts@.len(),
        ledger.accounts[index].phase != BudgetAccountPhase::Open,
    ensures !super::open_parent_chain(ledger, index),
{
    if super::open_parent_chain(ledger, index) {
        let path = choose |path: Seq<int>| #![auto]
            super::open_lineage_path(ledger, index, path);
        assert(path.len() > 0);
        assert(path[0] == index);
    }
}

pub(crate) proof fn open_chain_implies_parent_chain(
    ledger: &BudgetLedger,
    index: int,
    parent: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        0 <= parent < index < ledger.accounts@.len(),
        crate::identity_model::parent_matches(
            ledger.accounts[index].parent_id,
            ledger.accounts[parent].id,
        ),
        super::open_parent_chain(ledger, index),
    ensures super::open_parent_chain(ledger, parent),
{
    let path = choose |path: Seq<int>| #![auto]
        super::open_lineage_path(ledger, index, path);
    assert(path.len() > 1) by {
        if path.len() == 1 {
            assert(path[path.len() - 1] == index);
            assert(ledger.accounts[index].parent_id.is_none());
        }
    }
    assert(path[0] == index);
    assert(0 <= path[1] < index);
    assert(crate::identity_model::parent_matches(
        ledger.accounts[index].parent_id,
        ledger.accounts[path[1]].id,
    ));
    assert(crate::identity_model::budget_ids_equal(
        ledger.accounts[parent].id,
        ledger.accounts[path[1]].id,
    ));
    crate::invariant::matching_accounts_are_unique(ledger, parent, path[1]);
    let suffix = path.subrange(1, path.len() as int);
    assert(suffix.len() == path.len() - 1);
    assert(suffix[0] == parent);
    assert forall |position: int| #![auto]
        0 <= position < suffix.len() implies
            0 <= suffix[position] < ledger.accounts@.len()
                && ledger.accounts[suffix[position]].phase == BudgetAccountPhase::Open by {
        assert(suffix[position] == path[position + 1]);
    }
    assert forall |position: int| #![auto]
        0 <= position && position + 1 < suffix.len() implies
            0 <= suffix[position + 1] < suffix[position]
                && crate::identity_model::parent_matches(
                    ledger.accounts[suffix[position]].parent_id,
                    ledger.accounts[suffix[position + 1]].id,
                ) by {
        assert(suffix[position] == path[position + 1]);
        assert(suffix[position + 1] == path[position + 2]);
    }
    assert(suffix[suffix.len() - 1] == path[path.len() - 1]);
    assert(super::open_lineage_path(ledger, parent, suffix));
}

pub(crate) proof fn account_without_chain_has_no_lineage(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        super::account_at(ledger, budget_id, index),
        !super::open_parent_chain(ledger, index),
    ensures !super::lineage_is_open(ledger, budget_id),
{
    if super::lineage_is_open(ledger, budget_id) {
        let target = choose |target: int| #![auto]
            super::account_at(ledger, budget_id, target)
                && super::open_parent_chain(ledger, target);
        assert(crate::identity_model::budget_ids_equal(
            ledger.accounts[index].id,
            ledger.accounts[target].id,
        ));
        crate::invariant::matching_accounts_are_unique(ledger, index, target);
        assert(false);
    }
}

pub(crate) proof fn absent_account_has_no_lineage(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
)
    requires forall |index: int| #![auto]
        0 <= index < ledger.accounts@.len()
            ==> !crate::identity_model::budget_ids_equal(
                ledger.accounts[index].id,
                budget_id,
            ),
    ensures !super::lineage_is_open(ledger, budget_id),
{
    if super::lineage_is_open(ledger, budget_id) {
        let target = choose |target: int| #![auto]
            super::account_at(ledger, budget_id, target)
                && super::open_parent_chain(ledger, target);
        assert(false);
    }
}

} // verus!
