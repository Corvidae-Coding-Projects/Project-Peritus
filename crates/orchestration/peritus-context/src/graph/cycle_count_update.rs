//! Edge-driven maintenance of exact remaining-dependent counts.

#[cfg(verus_only)]
use super::cycle_count_model::{
    counts_match, present_source_makes_remaining_positive, remaining_count_from,
    removing_source_updates_count,
};
#[cfg(verus_only)]
use super::cycle_edge_model::{
    dependency_match_count_from, dependency_match_count_is_indicator,
    matching_index_makes_full_count_positive,
};
#[cfg(verus_only)]
use super::{cycle_model, model};
use super::validation;
use crate::ContextNode;
use vstd::prelude::*;

verus! {

pub(super) fn remove_source(
    nodes: &[ContextNode],
    _removed: &[bool],
    counts: &mut Vec<usize>,
    source: usize,
)
    requires
        model::nodes_canonical(nodes@),
        model::dependencies_exist(nodes@),
        counts_match(nodes@, _removed@, old(counts)@),
        source < nodes@.len(),
        !_removed@[source as int],
    ensures counts_match(
        nodes@,
        _removed@.update(source as int, true),
        final(counts)@,
    ),
{
    let ghost before = counts@;
    let source_node = &nodes[source];
    let dependencies = source_node.dependencies();
    proof {
        use_type_invariant(&*source_node);
        assert(source_node.invariant());
        assert(nodes@[source as int].invariant());
        reveal(counts_match);
    }
    let mut dependency = 0usize;
    while dependency < dependencies.len()
        invariant
            dependency <= dependencies.len(),
            source < nodes@.len(),
            !_removed@[source as int],
            dependencies@ == nodes@[source as int].spec_dependencies(),
            nodes@[source as int].invariant(),
            counts@.len() == nodes@.len(),
            before.len() == nodes@.len(),
            _removed@.len() == nodes@.len(),
            model::nodes_canonical(nodes@),
            model::dependencies_exist(nodes@),
            forall |target: int| #![auto] 0 <= target < nodes@.len() ==>
                before[target] as nat == remaining_count_from(
                    nodes@,
                    _removed@,
                    target as nat,
                    0,
                ),
            forall |target: int| #![auto] 0 <= target < nodes@.len() ==>
                counts@[target] as nat
                    + dependency_match_count_from(
                        dependencies@,
                        nodes@[target].spec_id(),
                        0,
                    )
                    == before[target] as nat
                        + dependency_match_count_from(
                            dependencies@,
                            nodes@[target].spec_id(),
                            dependency as nat,
                        ),
        decreases dependencies.len() - dependency,
    {
        proof {
            model::admitted_dependency_exists(nodes@, source as nat, dependency as nat);
        }
        let Some(target) = validation::find_node_index(nodes, dependencies[dependency]) else {
            proof {
                assert(model::node_exists(nodes@, dependencies@[dependency as int]));
                assert(false);
            }
            return;
        };
        let ghost before_set = counts@;
        proof {
            decrement_is_safe(
                nodes@, _removed@, dependencies@, counts@, before,
                source as nat, dependency as nat, target as nat,
            );
        }
        let next = counts[target] - 1;
        counts.set(target, next);
        proof {
            assert(counts@ == before_set.update(target as int, next));
            assert(next + 1 == before_set[target as int]);
            decrement_step(
                nodes@, dependencies@, dependency as nat, target as nat,
                before, before_set, counts@, next,
            );
        }
        dependency += 1;
    }
    proof {
        finish_removed_source(
            nodes@, _removed@, counts@, before,
            source as nat, dependencies@,
        );
    };
}

proof fn decrement_is_safe(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    dependencies: Seq<crate::ContextNodeId>,
    counts: Seq<usize>,
    before: Seq<usize>,
    source: nat,
    dependency: nat,
    target: nat,
)
    requires
        source < nodes.len(),
        target < nodes.len(),
        dependency < dependencies.len(),
        removed.len() == nodes.len(),
        !removed[source as int],
        dependencies == nodes[source as int].spec_dependencies(),
        nodes[source as int].invariant(),
        dependencies[dependency as int].spec_matches(&nodes[target as int].spec_id()),
        before[target as int] as nat
            == remaining_count_from(nodes, removed, target, 0),
        counts[target as int] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[target as int].spec_id(),
                0,
            )
            == before[target as int] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[target as int].spec_id(),
                    dependency,
                ),
    ensures counts[target as int] > 0,
{
    dependency_match_count_is_indicator(
        &nodes[source as int],
        nodes[target as int].spec_id(),
    );
    matching_index_makes_full_count_positive(
        dependencies,
        nodes[target as int].spec_id(),
        dependency,
    );
    assert(dependency_match_count_from(
        dependencies,
        nodes[target as int].spec_id(),
        0,
    ) == 1);
    reveal(cycle_model::node_depends_on);
    assert(cycle_model::node_depends_on(nodes, source, target));
    present_source_makes_remaining_positive(nodes, removed, target, source);
    assert(before[target as int] > 0);
    reveal(dependency_match_count_from);
}

proof fn decrement_step(
    nodes: Seq<ContextNode>,
    dependencies: Seq<crate::ContextNodeId>,
    dependency: nat,
    target: nat,
    before: Seq<usize>,
    before_set: Seq<usize>,
    after: Seq<usize>,
    next: usize,
)
    requires
        dependency < dependencies.len(),
        target < nodes.len(),
        nodes.len() == before_set.len(),
        model::nodes_canonical(nodes),
        dependencies[dependency as int].spec_matches(&nodes[target as int].spec_id()),
        after == before_set.update(target as int, next),
        next + 1 == before_set[target as int],
        forall |other: int| #![auto] 0 <= other < nodes.len() ==>
            before_set[other] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    0,
                )
                == before[other] as nat
                    + dependency_match_count_from(
                        dependencies,
                        nodes[other].spec_id(),
                        dependency,
                    ),
    ensures forall |other: int| #![auto] 0 <= other < nodes.len() ==>
        after[other] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                0,
            )
            == before[other] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    dependency + 1,
                ),
{
    assert forall |other: int| #![auto] 0 <= other < nodes.len() implies
        after[other] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                0,
            )
            == before[other] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    dependency + 1,
                ) by {
        reveal(dependency_match_count_from);
        if other == target {
        } else if dependencies[dependency as int].spec_matches(
            &nodes[other].spec_id(),
        ) {
            model::canonical_match_is_unique(
                nodes,
                dependencies[dependency as int],
                target,
                other as nat,
            );
            assert(false);
        }
    }
}

proof fn finish_removed_source(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    counts: Seq<usize>,
    before: Seq<usize>,
    source: nat,
    dependencies: Seq<crate::ContextNodeId>,
)
    requires
        source < nodes.len(),
        !removed[source as int],
        removed.len() == nodes.len(),
        counts.len() == nodes.len(),
        before.len() == nodes.len(),
        dependencies == nodes[source as int].spec_dependencies(),
        nodes[source as int].invariant(),
        forall |target: int| #![auto] 0 <= target < nodes.len() ==>
            before[target] as nat == remaining_count_from(
                nodes,
                removed,
                target as nat,
                0,
            ),
        forall |target: int| #![auto] 0 <= target < nodes.len() ==>
            counts[target] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[target].spec_id(),
                    0,
                )
                == before[target] as nat
                    + dependency_match_count_from(
                        dependencies,
                        nodes[target].spec_id(),
                        dependencies.len(),
                    ),
    ensures counts_match(
        nodes,
        removed.update(source as int, true),
        counts,
    ),
{
    assert forall |target: int| #![auto] 0 <= target < nodes.len() implies
        counts[target] as nat == remaining_count_from(
            nodes,
            removed.update(source as int, true),
            target as nat,
            0,
        ) by {
        dependency_match_count_is_indicator(
            &nodes[source as int],
            nodes[target].spec_id(),
        );
        removing_source_updates_count(nodes, removed, source, target as nat, 0);
        assert(dependency_match_count_from(
            dependencies,
            nodes[target].spec_id(),
            dependencies.len(),
        ) == 0) by {
            reveal(dependency_match_count_from);
        }
        assert(counts[target] as nat
            + dependency_match_count_from(dependencies, nodes[target].spec_id(), 0)
            == before[target] as nat);
        reveal(cycle_model::node_depends_on);
        if cycle_model::dependency_contains_from(
            dependencies,
            nodes[target].spec_id(),
            0,
        ) {
            assert(dependency_match_count_from(
                dependencies,
                nodes[target].spec_id(),
                0,
            ) == 1);
            assert(cycle_model::node_depends_on(nodes, source, target as nat));
            assert(counts[target] as nat + 1 == before[target] as nat);
            assert(before[target] as nat == remaining_count_from(
                nodes,
                removed.update(source as int, true),
                target as nat,
                0,
            ) + 1);
        } else {
            assert(dependency_match_count_from(
                dependencies,
                nodes[target].spec_id(),
                0,
            ) == 0);
            assert(!cycle_model::node_depends_on(nodes, source, target as nat));
            assert(counts[target] as nat == before[target] as nat);
            assert(before[target] as nat == remaining_count_from(
                nodes,
                removed.update(source as int, true),
                target as nat,
                0,
            ));
        }
    }
    reveal(counts_match);
}

} // verus!
