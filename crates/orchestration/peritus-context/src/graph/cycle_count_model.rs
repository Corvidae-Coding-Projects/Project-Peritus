//! Exact remaining-dependent-count model and update lemmas.

#[cfg(verus_only)]
use super::cycle_model;
#[cfg(verus_only)]
use crate::{ContextNode, ContextNodeId};
use vstd::prelude::*;

verus! {

/// Number of not-yet-removed sources depending directly on one target.
pub open spec fn remaining_count_from(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
) -> nat
    decreases nodes.len() - source,
{
    if source >= nodes.len() {
        0
    } else {
        (if !removed[source as int] && cycle_model::node_depends_on(nodes, source, target) {
            1nat
        } else {
            0nat
        }) + remaining_count_from(nodes, removed, target, source + 1)
    }
}

/// Exact correspondence between an executable count vector and a removal state.
pub open spec fn counts_match(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    counts: Seq<usize>,
) -> bool {
    removed.len() == nodes.len()
        && counts.len() == nodes.len()
        && forall |target: int| #![auto] 0 <= target < nodes.len() ==>
            counts[target] as nat == remaining_count_from(
                nodes,
                removed,
                target as nat,
                0,
            )
}

proof fn present_source_makes_remaining_positive_from(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
    scan: nat,
)
    requires
        removed.len() == nodes.len(),
        target < nodes.len(),
        scan <= source < nodes.len(),
        !removed[source as int],
        cycle_model::node_depends_on(nodes, source, target),
    ensures remaining_count_from(nodes, removed, target, scan) > 0,
    decreases source - scan,
{
    reveal(remaining_count_from);
    if scan < source {
        present_source_makes_remaining_positive_from(
            nodes,
            removed,
            target,
            source,
            scan + 1,
        );
    }
}

/// A present direct dependent contributes positively to the target's exact count.
pub proof fn present_source_makes_remaining_positive(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
)
    requires
        removed.len() == nodes.len(),
        target < nodes.len(),
        source < nodes.len(),
        !removed[source as int],
        cycle_model::node_depends_on(nodes, source, target),
    ensures remaining_count_from(nodes, removed, target, 0) > 0,
{
    present_source_makes_remaining_positive_from(
        nodes,
        removed,
        target,
        source,
        0,
    );
}

/// A zero count is exactly the absence of a remaining dependent.
pub proof fn zero_iff_no_remaining(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
)
    requires
        removed.len() == nodes.len(),
        target < nodes.len(),
        source <= nodes.len(),
    ensures
        remaining_count_from(nodes, removed, target, source) == 0
            <==> cycle_model::first_remaining_dependent_from(
                nodes,
                removed,
                target,
                source,
            ).is_none(),
    decreases nodes.len() - source,
{
    reveal(remaining_count_from);
    reveal(cycle_model::first_remaining_dependent_from);
    if source < nodes.len() {
        zero_iff_no_remaining(nodes, removed, target, source + 1);
    }
}

/// Removing one present source subtracts exactly its contribution to a target count.
pub proof fn removing_source_updates_count(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    source: nat,
    target: nat,
    scan: nat,
)
    requires
        removed.len() == nodes.len(),
        source < nodes.len(),
        target < nodes.len(),
        scan <= nodes.len(),
        !removed[source as int],
    ensures
        remaining_count_from(nodes, removed, target, scan)
            == remaining_count_from(
                nodes,
                removed.update(source as int, true),
                target,
                scan,
            ) + if scan <= source && cycle_model::node_depends_on(nodes, source, target) {
                1nat
            } else {
                0nat
            },
    decreases nodes.len() - scan,
{
    reveal(remaining_count_from);
    if scan < nodes.len() {
        if scan == source {
            assert(removed.update(source as int, true)[scan as int]);
            assert(remaining_count_from(
                nodes,
                removed,
                target,
                scan + 1,
            ) == remaining_count_from(
                nodes,
                removed.update(source as int, true),
                target,
                scan + 1,
            )) by {
                removing_source_updates_count(
                    nodes,
                    removed,
                    source,
                    target,
                    scan + 1,
                );
            }
        } else {
            assert(removed.update(source as int, true)[scan as int] == removed[scan as int]);
            removing_source_updates_count(
                nodes,
                removed,
                source,
                target,
                scan + 1,
            );
        }
    }
}

/// All-present state has the direct-dependent count computed by a source scan.
pub proof fn initial_count_step(
    nodes: Seq<ContextNode>,
    target: nat,
    source: nat,
)
    requires target < nodes.len(), source <= nodes.len(),
    ensures
        remaining_count_from(
            nodes,
            cycle_model::initial_removed(nodes.len()),
            target,
            source,
        ) == if source >= nodes.len() {
            0nat
        } else {
            (if cycle_model::node_depends_on(nodes, source, target) {
                1nat
            } else {
                0nat
            }) + remaining_count_from(
                nodes,
                cycle_model::initial_removed(nodes.len()),
                target,
                source + 1,
            )
        },
{
    reveal(remaining_count_from);
    reveal(cycle_model::initial_removed);
}

} // verus!
