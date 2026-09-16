//! Canonical finite identity set used by cancellation traversal.

use crate::WorkId;
use vstd::prelude::*;

verus! {

pub(super) open spec fn ordered(values: Seq<WorkId>) -> bool {
    forall |left: int, right: int| 0 <= left < right < values.len() ==>
        values[left].spec_precedes(&values[right])
}

fn lower_bound(values: &[WorkId], id: WorkId) -> (at: usize)
    requires ordered(values@),
    ensures
        at <= values.len(),
        forall |index: int| 0 <= index < at ==> values@[index].spec_precedes(&id),
        forall |index: int| at <= index < values.len() ==> !values@[index].spec_precedes(&id),
{
    let mut low = 0;
    let mut high = values.len();
    while low < high
        invariant
            ordered(values@),
            low <= high <= values.len(),
            forall |index: int| 0 <= index < low ==> values@[index].spec_precedes(&id),
            forall |index: int| high <= index < values.len() ==> !values@[index].spec_precedes(&id),
        decreases high - low,
    {
        let middle = low + (high - low) / 2;
        let observed = values[middle];
        let goes_right = observed.precedes(&id);
        if goes_right {
            low = middle + 1;
        } else {
            high = middle;
        }
        proof {
            if goes_right {
                assert forall |index: int| 0 <= index <= middle implies
                    values@[index].spec_precedes(&id) by {
                    if index < middle {
                        WorkId::order_transitive(&values@[index], &observed, &id);
                    }
                }
            } else {
                assert forall |index: int| middle <= index < values.len() implies
                    !values@[index].spec_precedes(&id) by {
                    if middle < index && values@[index].spec_precedes(&id) {
                        WorkId::order_transitive(&observed, &values@[index], &id);
                    }
                }
            }
        };
    }
    low
}

pub(super) fn contains(values: &[WorkId], id: WorkId) -> (found: bool)
    requires ordered(values@),
    ensures found == values@.contains(id),
{
    let at = lower_bound(values, id);
    if at < values.len() && values[at].same(&id) {
        true
    } else {
        proof {
            assert forall |index: int| 0 <= index < values.len() implies values@[index] != id by {
                if index < at {
                    if values@[index] == id { WorkId::order_irreflexive(&id); }
                } else if at < index {
                    if values@[index] == id {
                        assert(values@[at as int].spec_precedes(&id));
                    }
                }
            }
        }
        false
    }
}

pub(super) fn insert(values: &mut Vec<WorkId>, id: WorkId) -> (inserted: bool)
    requires ordered(old(values)@),
    ensures
        ordered(final(values)@),
        final(values)@.to_set() == old(values)@.to_set().insert(id),
        forall |value: WorkId| final(values)@.contains(value)
            == (old(values)@.contains(value) || value == id),
        inserted == !old(values)@.contains(id),
        final(values).len() == old(values).len() + if inserted { 1int } else { 0int },
        !inserted ==> final(values)@ == old(values)@,
{
    let at = lower_bound(values, id);
    if at < values.len() && values[at].same(&id) {
        proof { assert(values@.to_set().insert(id) =~= values@.to_set()); }
        return false;
    }
    let ghost before = values@;
    proof {
        assert forall |index: int| 0 <= index < before.len() implies before[index] != id by {
            if index < at && before[index] == id { WorkId::order_irreflexive(&id); }
            if at < index && before[index] == id {
                assert(before[at as int].spec_precedes(&id));
            }
        }
    }
    values.insert(at, id);
    proof {
        let after = values@;
        assert forall |left: int, right: int| 0 <= left < right < after.len() implies
            after[left].spec_precedes(&after[right]) by {
            if left == at {
                let later = before[right - 1];
                WorkId::order_total(&id, &later);
            } else if right == at {
                assert(before[left].spec_precedes(&id));
            } else {
                let prior_left = if left < at { left } else { left - 1 };
                let prior_right = if right < at { right } else { right - 1 };
                assert(0 <= prior_left < prior_right < before.len());
            }
        }
        assert forall |value: WorkId| after.contains(value) <==>
                (before.contains(value) || value == id) by {
                if after.contains(value) {
                    let index = choose |index: int| 0 <= index < after.len() && after[index] == value;
                    if index != at {
                        let prior = if index < at { index } else { index - 1 };
                        assert(before[prior] == value);
                    }
                }
                if before.contains(value) {
                    let prior = choose |index: int| 0 <= index < before.len() && before[index] == value;
                    let index = if prior < at { prior } else { prior + 1 };
                    assert(after[index] == value);
                }
                if value == id {
                    assert(after[at as int] == value);
                }
        }
        assert(after.to_set() =~= before.to_set().insert(id));
    }
    true
}

} // verus!
