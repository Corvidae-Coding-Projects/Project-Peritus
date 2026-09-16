//! Exact zero-count selection for the production Kahn traversal.

#[cfg(verus_only)]
use super::cycle_count_model::{counts_match, remaining_count_from, zero_iff_no_remaining};
#[cfg(verus_only)]
use super::cycle_model;
use crate::ContextNode;
use vstd::prelude::*;

verus! {

pub(super) fn first_removable(
    nodes: &[ContextNode],
    removed: &[bool],
    counts: &[usize],
) -> (result: Option<usize>)
    requires counts_match(nodes@, removed@, counts@),
    ensures match result {
        Some(index) => index < nodes.len()
            && cycle_model::removable(nodes@, removed@, index as nat)
            && cycle_model::first_removable_from(nodes@, removed@, 0)
                == Some(index as nat),
        None => cycle_model::first_removable_from(nodes@, removed@, 0).is_none(),
    },
{
    reveal(counts_match);
    let mut index = 0usize;
    while index < nodes.len()
        invariant
            index <= nodes.len(),
            removed@.len() == nodes@.len(),
            counts@.len() == nodes@.len(),
            forall |target: int| #![auto] 0 <= target < nodes@.len() ==>
                counts@[target] as nat == remaining_count_from(
                    nodes@,
                    removed@,
                    target as nat,
                    0,
                ),
            cycle_model::first_removable_from(nodes@, removed@, 0)
                == cycle_model::first_removable_from(
                    nodes@,
                    removed@,
                    index as nat,
                ),
        decreases nodes.len() - index,
    {
        proof {
            zero_iff_no_remaining(nodes@, removed@, index as nat, 0);
            reveal(cycle_model::removable);
        }
        reveal(cycle_model::first_removable_from);
        if !removed[index] && counts[index] == 0 {
            assert(cycle_model::removable(nodes@, removed@, index as nat));
            return Some(index);
        }
        assert(!cycle_model::removable(nodes@, removed@, index as nat));
        index += 1;
    }
    None
}

} // verus!
