//! Edge-driven initialization of exact remaining-dependent counts.

#[cfg(verus_only)]
use super::cycle_count_model::counts_match;
#[cfg(verus_only)]
use super::cycle_edge_model::{
    dependency_match_count_from, dependency_match_count_is_indicator,
    matching_index_makes_full_count_positive, processed_source_count,
    processed_source_count_is_bounded, processed_sources_partition_initial,
};
#[cfg(verus_only)]
use super::{cycle_model, model};
use super::validation;
use crate::ContextNode;
use vstd::prelude::*;

verus! {

pub(super) fn initial_counts(nodes: &[ContextNode]) -> (counts: Vec<usize>)
    requires
        model::nodes_canonical(nodes@),
        model::dependencies_exist(nodes@),
    ensures counts_match(nodes@, cycle_model::initial_removed(nodes@.len()), counts@),
{
    let mut counts = Vec::with_capacity(nodes.len());
    while counts.len() < nodes.len()
        invariant
            counts.len() <= nodes.len(),
            forall |target: int| #![auto] 0 <= target < counts@.len() ==>
                counts@[target] == 0,
        decreases nodes.len() - counts.len(),
    {
        counts.push(0usize);
    }

    let mut source = 0usize;
    while source < nodes.len()
        invariant
            source <= nodes.len(),
            counts@.len() == nodes@.len(),
            model::nodes_canonical(nodes@),
            model::dependencies_exist(nodes@),
            forall |target: int| #![auto] 0 <= target < nodes@.len() ==>
                counts@[target] as nat
                    == processed_source_count(nodes@, target as nat, source as nat),
        decreases nodes.len() - source,
    {
        let source_node = &nodes[source];
        let dependencies = source_node.dependencies();
        proof {
            use_type_invariant(&*source_node);
            assert(source_node.invariant());
            assert(nodes@[source as int].invariant());
        }
        let mut dependency = 0usize;
        while dependency < dependencies.len()
            invariant
                dependency <= dependencies.len(),
                source < nodes@.len(),
                dependencies@ == nodes@[source as int].spec_dependencies(),
                counts@.len() == nodes@.len(),
                model::nodes_canonical(nodes@),
                model::dependencies_exist(nodes@),
                nodes@[source as int].invariant(),
                forall |target: int| #![auto] 0 <= target < nodes@.len() ==>
                    counts@[target] as nat
                        + dependency_match_count_from(
                            dependencies@,
                            nodes@[target].spec_id(),
                            dependency as nat,
                        )
                        == processed_source_count(nodes@, target as nat, source as nat)
                            + dependency_match_count_from(
                                dependencies@,
                                nodes@[target].spec_id(),
                                0,
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
                return counts;
            };
            let ghost before = counts@;
            proof {
                increment_is_safe(
                    nodes@, dependencies@, counts@, source as nat,
                    dependency as nat, target as nat,
                );
            }
            let next = counts[target] + 1;
            counts.set(target, next);
            proof {
                assert(counts@ == before.update(target as int, next));
                increment_step(
                    nodes@, dependencies@, dependency as nat, target as nat,
                    before, counts@, next, source as nat,
                );
            }
            dependency += 1;
        }
        proof {
            finish_source(nodes@, dependencies@, counts@, source as nat);
        }
        source += 1;
    }
    proof {
        finish_initial_counts(nodes@, counts@);
    }
    counts
}

proof fn increment_is_safe(
    nodes: Seq<ContextNode>,
    dependencies: Seq<crate::ContextNodeId>,
    counts: Seq<usize>,
    source: nat,
    dependency: nat,
    target: nat,
)
    requires
        source < nodes.len(),
        target < nodes.len(),
        dependency < dependencies.len(),
        counts.len() == nodes.len(),
        nodes.len() <= usize::MAX,
        dependencies == nodes[source as int].spec_dependencies(),
        nodes[source as int].invariant(),
        dependencies[dependency as int].spec_matches(&nodes[target as int].spec_id()),
        counts[target as int] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[target as int].spec_id(),
                dependency,
            )
            == processed_source_count(nodes, target, source)
                + dependency_match_count_from(
                    dependencies,
                    nodes[target as int].spec_id(),
                    0,
                ),
    ensures counts[target as int] < usize::MAX,
{
    assert(target < counts.len());
    dependency_match_count_is_indicator(
        &nodes[source as int],
        nodes[target as int].spec_id(),
    );
    processed_source_count_is_bounded(nodes, target, source);
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
    assert(counts[target as int] as nat <= processed_source_count(nodes, target, source));
    assert(processed_source_count(nodes, target, source) <= source);
    assert(source < usize::MAX);
}

proof fn increment_step(
    nodes: Seq<ContextNode>,
    dependencies: Seq<crate::ContextNodeId>,
    dependency: nat,
    target: nat,
    before: Seq<usize>,
    after: Seq<usize>,
    next: usize,
    source: nat,
)
    requires
        source < nodes.len(),
        target < nodes.len(),
        dependency < dependencies.len(),
        before.len() == nodes.len(),
        after.len() == nodes.len(),
        model::nodes_canonical(nodes),
        dependencies[dependency as int].spec_matches(&nodes[target as int].spec_id()),
        after == before.update(target as int, next),
        next == before[target as int] + 1,
        forall |other: int| #![auto] 0 <= other < nodes.len() ==>
            before[other] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    dependency,
                )
                == processed_source_count(nodes, other as nat, source)
                    + dependency_match_count_from(
                        dependencies,
                        nodes[other].spec_id(),
                        0,
                    ),
    ensures forall |other: int| #![auto] 0 <= other < nodes.len() ==>
        after[other] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                dependency + 1,
            )
            == processed_source_count(nodes, other as nat, source)
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    0,
                ),
{
    assert forall |other: int| #![auto] 0 <= other < nodes.len() implies
        after[other] as nat
            + dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                dependency + 1,
            )
            == processed_source_count(nodes, other as nat, source)
                + dependency_match_count_from(
                    dependencies,
                    nodes[other].spec_id(),
                    0,
        ) by {
        reveal(dependency_match_count_from);
        if other == target {
            assert(after[other] == next);
            assert(before[other] == before[target as int]);
            assert(dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                dependency,
            ) == 1 + dependency_match_count_from(
                dependencies,
                nodes[other].spec_id(),
                dependency + 1,
            ));
        } else if dependencies[dependency as int].spec_matches(&nodes[other].spec_id()) {
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

proof fn finish_source(
    nodes: Seq<ContextNode>,
    dependencies: Seq<crate::ContextNodeId>,
    counts: Seq<usize>,
    source: nat,
)
    requires
        source < nodes.len(),
        counts.len() == nodes.len(),
        dependencies == nodes[source as int].spec_dependencies(),
        nodes[source as int].invariant(),
        forall |target: int| #![auto] 0 <= target < nodes.len() ==>
            counts[target] as nat
                + dependency_match_count_from(
                    dependencies,
                    nodes[target].spec_id(),
                    dependencies.len(),
                )
                == processed_source_count(nodes, target as nat, source)
                    + dependency_match_count_from(
                        dependencies,
                        nodes[target].spec_id(),
                        0,
                    ),
    ensures forall |target: int| #![auto] 0 <= target < nodes.len() ==>
        counts[target] as nat
            == processed_source_count(nodes, target as nat, source + 1),
{
    assert forall |target: int| #![auto] 0 <= target < nodes.len() implies
        counts[target] as nat
            == processed_source_count(nodes, target as nat, source + 1) by {
        dependency_match_count_is_indicator(
            &nodes[source as int],
            nodes[target].spec_id(),
        );
        reveal(processed_source_count);
        reveal(cycle_model::node_depends_on);
        reveal(dependency_match_count_from);
    }
}

proof fn finish_initial_counts(nodes: Seq<ContextNode>, counts: Seq<usize>)
    requires
        counts.len() == nodes.len(),
        forall |target: int| #![auto] 0 <= target < nodes.len() ==>
            counts[target] as nat
                == processed_source_count(nodes, target as nat, nodes.len()),
    ensures counts_match(
        nodes,
        cycle_model::initial_removed(nodes.len()),
        counts,
    ),
{
    assert forall |target: int| #![auto] 0 <= target < nodes.len() implies
        counts[target] as nat == super::cycle_count_model::remaining_count_from(
            nodes,
            cycle_model::initial_removed(nodes.len()),
            target as nat,
            0,
        ) by {
        processed_sources_partition_initial(nodes, target as nat, nodes.len());
        reveal(super::cycle_count_model::remaining_count_from);
    }
    reveal(counts_match);
}

} // verus!
