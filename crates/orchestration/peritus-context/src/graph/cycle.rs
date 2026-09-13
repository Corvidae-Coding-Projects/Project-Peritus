//! Production Kahn traversal connected to the finite cycle model.

#[cfg(verus_only)]
use super::cycle_count_model;
use super::cycle_count_init;
use super::cycle_count_update;
use super::cycle_counts;
#[cfg(verus_only)]
use super::cycle_model;
#[cfg(verus_only)]
use super::cycle_proofs;
#[cfg(verus_only)]
use super::model;
use crate::{ContextNode, ContextNodeId};
use vstd::prelude::*;

verus! {

fn first_unremoved(removed: &[bool]) -> (result: Option<usize>)
    ensures match result {
        Some(index) => index < removed.len()
            && !removed@[index as int]
            && cycle_model::first_unremoved_from(removed@, 0) == Some(index as nat),
        None => cycle_model::first_unremoved_from(removed@, 0).is_none(),
    },
{
    let mut index = 0;
    while index < removed.len()
        invariant
            index <= removed.len(),
            cycle_model::first_unremoved_from(removed@, 0)
                == cycle_model::first_unremoved_from(removed@, index as nat),
        decreases removed.len() - index,
    {
        if !removed[index] {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn initial_removal_state(node_count: usize) -> (removed: Vec<bool>)
    ensures removed@ == cycle_model::initial_removed(node_count as nat),
{
    let mut removed: Vec<bool> = Vec::with_capacity(node_count);
    while removed.len() < node_count
        invariant
            removed.len() <= node_count,
            forall |index: int| 0 <= index < removed@.len() ==> !removed@[index],
        decreases node_count - removed.len(),
    {
        removed.push(false);
    }
    assert(removed@ =~= cycle_model::initial_removed(node_count as nat));
    removed
}

pub(super) fn cycle_member(nodes: &[ContextNode]) -> (result: Option<ContextNodeId>)
    requires model::nodes_canonical(nodes@), model::dependencies_exist(nodes@),
    ensures cycle_model::cycle_result(nodes@, result),
        result.is_none() == cycle_model::acyclic(nodes@),
{
    let mut removed = initial_removal_state(nodes.len());
    let mut dependent_counts = cycle_count_init::initial_counts(nodes);
    let mut removed_count = 0usize;
    let ghost mut proof_order: Seq<nat> = Seq::empty();
    proof {
        cycle_model::initial_state_matches(nodes@.len());
        reveal(cycle_model::canonical_elimination);
    }
    let mut exhausted = false;
    while removed_count < nodes.len() && !exhausted
        invariant
            model::nodes_canonical(nodes@),
            model::dependencies_exist(nodes@),
            removed@.len() == nodes@.len(),
            removed_count <= nodes@.len(),
            removed_count == proof_order.len(),
            cycle_model::removal_state_matches(removed@, proof_order, nodes@.len()),
            cycle_count_model::counts_match(nodes@, removed@, dependent_counts@),
            cycle_model::partial_elimination_certificate(nodes@, proof_order),
            exhausted ==> cycle_model::first_removable_from(nodes@, removed@, 0).is_none(),
            cycle_model::eliminate(
                nodes@,
                removed@,
                proof_order,
                (nodes@.len() - removed_count) as int,
            ) == cycle_model::canonical_elimination(nodes@),
        decreases nodes.len() - removed_count, if exhausted { 0nat } else { 1nat },
    {
        match cycle_counts::first_removable(
            nodes, removed.as_slice(), dependent_counts.as_slice(),
        ) {
            None => {
                exhausted = true;
            }
            Some(index) => {
                let ghost old_removed = removed@;
                let ghost old_proof_order = proof_order;
                proof {
                    cycle_model::append_removable(
                        nodes@, old_removed, old_proof_order, index as nat,
                    );
                    reveal(cycle_model::eliminate);
                }
                cycle_count_update::remove_source(
                    nodes, removed.as_slice(), &mut dependent_counts, index,
                );
                removed.set(index, true);
                removed_count += 1;
                proof {
                    proof_order = old_proof_order.push(index as nat);
                }
                assert(removed@ == old_removed.update(index as int, true));
            }
        }
    }
    proof {
        cycle_proofs::final_state_is_canonical(
            nodes@,
            removed@,
            proof_order,
            removed_count as nat,
            exhausted,
        );
    }
    let first = first_unremoved(removed.as_slice());
    if first.is_none() {
        proof {
            cycle_model::complete_trace_has_full_length(
                removed@,
                proof_order,
                nodes@.len(),
            );
            assert(cycle_model::elimination_certificate(nodes@, proof_order));
            assert(cycle_model::acyclic(nodes@));
        }
        return None;
    }
    let index = first.unwrap();
    proof {
        if !exhausted {
            assert(removed_count == nodes@.len());
            cycle_proofs::full_trace_marks_every_node(
                removed@,
                proof_order,
                nodes@.len(),
            );
            assert(false);
        }
        if cycle_model::acyclic(nodes@) {
            acyclic_state_has_removable_contradiction(nodes@, removed@);
        }
    }
    Some(nodes[index].id())
}

proof fn acyclic_state_has_removable_contradiction(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
)
    requires
        cycle_model::acyclic(nodes),
        removed.len() == nodes.len(),
        cycle_model::first_unremoved_from(removed, 0).is_some(),
        cycle_model::first_removable_from(nodes, removed, 0).is_none(),
    ensures false,
{
    let first = cycle_model::first_unremoved_from(removed, 0).unwrap();
    assert(first < removed.len() && !removed[first as int]) by {
        first_unremoved_model_is_exact(removed, 0);
    }
    cycle_proofs::acyclic_state_has_removable_node(nodes, removed);
    cycle_proofs::no_first_removable_means_none_removable(nodes, removed, 0);
    let target = choose |target: nat| target < nodes.len()
        && cycle_model::removable(nodes, removed, target);
    assert(cycle_model::removable(nodes, removed, target));
    let target_index = target as int;
    assert(target_index as nat == target);
    assert(!cycle_model::removable(nodes, removed, target_index as nat));
}

proof fn first_unremoved_model_is_exact(removed: Seq<bool>, start: nat)
    requires
        start <= removed.len(),
        cycle_model::first_unremoved_from(removed, start).is_some(),
    ensures {
        let index = cycle_model::first_unremoved_from(removed, start).unwrap();
        start <= index < removed.len() && !removed[index as int]
    },
    decreases removed.len() - start,
{
    reveal(cycle_model::first_unremoved_from);
    if start < removed.len() && removed[start as int] {
        first_unremoved_model_is_exact(removed, start + 1);
    }
}

} // verus!
