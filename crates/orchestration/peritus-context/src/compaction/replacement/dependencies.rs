//! Dependency collection and deterministic rewrite helpers.

use super::super::ValidatedCompaction;
#[cfg(verus_only)]
use super::super::ValidatedSource;
#[cfg(verus_only)]
use super::model::{
    contains_id, contains_id_is_sequence_membership, external_dependency_contains,
    external_dependency_contains_push, ids_match_iff_equal, rewritten_dependency_contains,
    rewritten_dependency_contains_push, source_external_dependency_contains,
    source_external_dependency_contains_push,
};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
#[allow(
    clippy::too_many_lines,
    reason = "nested finite traversals retain the source and dependency proof invariants together"
)]
pub(super) fn external_dependencies(
    validated: &ValidatedCompaction,
    source_ids: &[ContextNodeId],
) -> (dependencies: Vec<ContextNodeId>)
    ensures forall |id: ContextNodeId| contains_id(dependencies@, id) ==
        external_dependency_contains(validated.spec_source_nodes(), source_ids@, id),
{
    let mut dependencies = Vec::new();
    let mut source_index = 0;
    while source_index < validated.sources.len()
        invariant
            source_index <= validated.sources.len(),
            forall |id: ContextNodeId| contains_id(dependencies@, id) ==
                external_dependency_contains(
                    validated.spec_source_nodes().take(source_index as int), source_ids@, id,
                ),
        decreases validated.sources.len() - source_index,
    {
        let source = &validated.sources[source_index];
        let source_dependencies = source.node.dependencies();
        proof {
            assert(source_dependencies@ == source.spec_node().spec_dependencies());
        }
        let mut dependency_index = 0;
        while dependency_index < source_dependencies.len()
            invariant
                dependency_index <= source_dependencies.len(),
                forall |id: ContextNodeId| contains_id(dependencies@, id) ==
                    (external_dependency_contains(
                        validated.spec_source_nodes().take(source_index as int), source_ids@, id,
                    ) || source_external_dependency_contains(
                        source_dependencies@.take(dependency_index as int), source_ids@, id,
                    )),
            decreases source_dependencies.len() - dependency_index,
        {
            let dependency = source_dependencies[dependency_index];
            let is_source = contains(source_ids, dependency);
            let ghost prior_dependencies = dependencies@;
            if !is_source {
                insert_canonical(&mut dependencies, dependency);
            }
            proof {
                assert(dependency_index < source_dependencies@.len());
                assert(dependency == source_dependencies@[dependency_index as int]);
                assert(is_source == contains_id(source_ids@, dependency));
                assert(source_dependencies@.take(dependency_index as int).push(
                    source_dependencies@[dependency_index as int],
                ) =~= source_dependencies@.take(dependency_index as int + 1));
                reveal_with_fuel(source_external_dependency_contains, 1);
                assert forall |id: ContextNodeId| contains_id(dependencies@, id) ==
                    (external_dependency_contains(
                        validated.spec_source_nodes().take(source_index as int), source_ids@, id,
                    ) || source_external_dependency_contains(
                        source_dependencies@.take(dependency_index as int + 1), source_ids@, id,
                    )) by {
                        source_external_dependency_contains_push(
                            source_dependencies@.take(dependency_index as int),
                            source_ids@,
                            dependency,
                            id,
                        );
                        assert(contains_id(prior_dependencies, id) ==
                            (external_dependency_contains(
                                validated.spec_source_nodes().take(source_index as int),
                                source_ids@,
                                id,
                            ) || source_external_dependency_contains(
                                source_dependencies@.take(dependency_index as int),
                                source_ids@,
                                id,
                            )));
                        if is_source {
                            assert(dependencies@ == prior_dependencies);
                        } else {
                            assert(contains_id(dependencies@, id) ==
                                (contains_id(prior_dependencies, id) || dependency == id));
                        }
                    }
            }
            dependency_index += 1;
        }
        proof {
            assert(dependency_index == source_dependencies.len());
            assert(source_dependencies@.take(dependency_index as int)
                =~= source_dependencies@);
            assert(source_dependencies@ == source.spec_node().spec_dependencies());
            reveal(ValidatedCompaction::spec_source_nodes);
            assert(*source == validated.sources@[source_index as int]);
            assert(source.spec_node() == validated.spec_source_nodes()[source_index as int]);
            reveal_with_fuel(external_dependency_contains, 1);
            assert(validated.spec_source_nodes().take(source_index as int).push(
                source.spec_node(),
            ) =~= validated.spec_source_nodes().take(source_index as int + 1));
            assert forall |id: ContextNodeId| contains_id(dependencies@, id) ==
                external_dependency_contains(
                    validated.spec_source_nodes().take(source_index as int + 1), source_ids@, id,
                ) by {
                    external_dependency_contains_push(
                        validated.spec_source_nodes().take(source_index as int),
                        source_ids@,
                        source.spec_node(),
                        id,
                    );
                    assert(contains_id(dependencies@, id) ==
                        (external_dependency_contains(
                            validated.spec_source_nodes().take(source_index as int),
                            source_ids@,
                            id,
                        ) || source_external_dependency_contains(
                            source_dependencies@,
                            source_ids@,
                            id,
                        )));
                }
        }
        source_index += 1;
    }
    proof {
        assert(validated.spec_source_nodes().take(source_index as int)
            =~= validated.spec_source_nodes());
    }
    dependencies
}

pub(super) fn rewrite_dependencies(
    dependencies: &[ContextNodeId],
    source_ids: &[ContextNodeId],
    output_id: ContextNodeId,
) -> (rewritten: Vec<ContextNodeId>)
    ensures forall |id: ContextNodeId| contains_id(rewritten@, id) ==
        rewritten_dependency_contains(dependencies@, source_ids@, output_id, id),
{
    let mut rewritten = Vec::with_capacity(dependencies.len());
    let mut dependency_index = 0;
    while dependency_index < dependencies.len()
        invariant
            dependency_index <= dependencies.len(),
            forall |id: ContextNodeId| contains_id(rewritten@, id) ==
                rewritten_dependency_contains(
                    dependencies@.take(dependency_index as int), source_ids@, output_id, id,
                ),
        decreases dependencies.len() - dependency_index,
    {
        let original = dependencies[dependency_index];
        let is_source = contains(source_ids, original);
        let dependency = if is_source {
            output_id
        } else {
            original
        };
        let ghost prior_rewritten = rewritten@;
        insert_canonical(&mut rewritten, dependency);
        proof {
            assert(original == dependencies@[dependency_index as int]);
            assert(is_source == contains_id(source_ids@, original));
            assert(dependencies@.take(dependency_index as int).push(
                dependencies@[dependency_index as int],
            ) =~= dependencies@.take(dependency_index as int + 1));
            reveal_with_fuel(rewritten_dependency_contains, 1);
            assert forall |id: ContextNodeId| contains_id(rewritten@, id) ==
                rewritten_dependency_contains(
                    dependencies@.take(dependency_index as int + 1), source_ids@, output_id, id,
                ) by {
                    rewritten_dependency_contains_push(
                        dependencies@.take(dependency_index as int),
                        source_ids@,
                        output_id,
                        original,
                        id,
                    );
                    assert(contains_id(prior_rewritten, id) ==
                        rewritten_dependency_contains(
                            dependencies@.take(dependency_index as int),
                            source_ids@,
                            output_id,
                            id,
                        ));
                    assert(contains_id(rewritten@, id) ==
                        (contains_id(prior_rewritten, id) || dependency == id));
                }
        }
        dependency_index += 1;
    }
    proof {
        assert(dependencies@.take(dependency_index as int) =~= dependencies@);
    }
    rewritten
}

#[allow(
    clippy::branches_sharing_code,
    reason = "each insertion branch establishes a distinct membership proof"
)]
pub(super) fn insert_canonical(values: &mut Vec<ContextNodeId>, value: ContextNodeId)
    ensures
        final(values)@.to_set() == old(values)@.to_set().insert(value),
        forall |id: ContextNodeId| contains_id(final(values)@, id) ==
            (contains_id(old(values)@, id) || value == id),
{
    let ghost before = values@;
    let mut position = 0;
    while position < values.len()
        && values[position].canonical_order(&value) == core::cmp::Ordering::Less
        invariant
            position <= values.len(),
            values@ == before,
        decreases values.len() - position,
    {
        position += 1;
    }
    if position == values.len() || !values[position].matches(&value) {
        let ghost at = position as int;
        values.insert(position, value);
        proof {
            let after = values@;
            assert forall |id: ContextNodeId| contains_id(after, id) <==>
                    (contains_id(before, id) || value == id) by {
                reveal(contains_id);
                if contains_id(after, id) {
                    let index = choose |index: int| #![trigger after[index]]
                        0 <= index < after.len() && after[index].spec_matches(&id);
                    if index == at {
                        ids_match_iff_equal(value, id);
                    } else {
                        let prior = if index < at { index } else { index - 1 };
                        assert(before[prior].spec_matches(&id));
                    }
                }
                if contains_id(before, id) {
                    let prior = choose |index: int| #![trigger before[index]]
                        0 <= index < before.len() && before[index].spec_matches(&id);
                    let index = if prior < at { prior } else { prior + 1 };
                    assert(after[index].spec_matches(&id));
                }
                if value == id {
                    assert(after[at].spec_matches(&id));
                }
            }
        };
    } else {
        proof {
            ContextNodeId::matches_implies_equal(&values@[position as int], &value);
            assert(before.contains(value));
            assert forall |id: ContextNodeId| contains_id(values@, id) ==
                (contains_id(before, id) || value == id) by {
                contains_id_is_sequence_membership(before, id);
            }
        };
    }
    assert forall |id: ContextNodeId| values@.contains(id) <==>
            (before.contains(id) || value == id) by {
        contains_id_is_sequence_membership(values@, id);
        contains_id_is_sequence_membership(before, id);
    }
    assert(values@.to_set() =~= before.to_set().insert(value)) by {
        assert forall |id: ContextNodeId| values@.to_set().contains(id) ==
            before.to_set().insert(value).contains(id) by {
            assert(values@.to_set().contains(id) == values@.contains(id));
            assert(before.to_set().contains(id) == before.contains(id));
            assert(before.to_set().insert(value).contains(id) ==
                (before.to_set().contains(id) || value == id));
        }
    };
}

pub(super) fn contains(values: &[ContextNodeId], target: ContextNodeId) -> (contains: bool)
    ensures
        contains == contains_id(values@, target),
        contains == values@.contains(target),
{
    let mut index = 0;
    while index < values.len()
        invariant
            index <= values.len(),
            forall |prior: int| 0 <= prior < index ==>
                !values@[prior].spec_matches(&target),
        decreases values.len() - index,
    {
        if values[index].matches(&target) {
            proof { contains_id_is_sequence_membership(values@, target); }
            return true;
        }
        index += 1;
    }
    proof { contains_id_is_sequence_membership(values@, target); }
    false
}


} // verus!
