//! Exact graph construction after source validation.

use super::dependencies::{contains, external_dependencies, rewrite_dependencies};
#[cfg(verus_only)]
use super::model::{
    contains_id, exact_replacement_graph, external_dependency_contains,
    replacement_node_matches, rewritten_dependency_contains, survivor_count,
    survivor_count_push, surviving_node_matches,
};
use super::super::ValidatedCompaction;
use crate::{ContextError, ContextGraph, ContextNodeId};
use vstd::prelude::*;

verus! {

#[allow(clippy::too_many_lines, reason = "the graph rewrite and its proof are one transaction")]
pub(super) fn build_replacement_graph(
    graph: &ContextGraph,
    validated: &ValidatedCompaction,
    source_ids: &[ContextNodeId],
) -> (result: Result<ContextGraph, ContextError>)
    requires
        source_ids@ == validated.spec_source_ids(),
        !contains_id(source_ids@, validated.spec_node().spec_id()),
    ensures match result {
        Ok(after) => exact_replacement_graph(graph, validated, &after),
        Err(_) => true,
    },
{
    let output_id = validated.node.id();
    let live_dependencies = external_dependencies(validated, source_ids);
    let replacement = validated
        .node
        .replace_dependencies(live_dependencies, graph.limits())?;
    proof {
        reveal(replacement_node_matches);
        assert(crate::ContextNode::dependencies_replaced(
            &validated.spec_node(), &replacement, replacement.spec_dependencies(),
        ));
        assert forall |id: ContextNodeId| contains_id(replacement.spec_dependencies(), id) ==
            external_dependency_contains(
                validated.spec_source_nodes(), source_ids@, id,
            ) by {}
        assert(replacement_node_matches(
            &validated.spec_node(),
            &replacement,
            validated.spec_source_nodes(),
            source_ids@,
        ));
    }
    let graph_nodes = graph.nodes();
    let mut nodes = Vec::with_capacity(graph_nodes.len());
    let mut replacement_inserted = false;
    let mut node_index = 0;
    while node_index < graph_nodes.len()
        invariant
            node_index <= graph_nodes.len(),
            graph_nodes@ == graph.spec_nodes(),
            source_ids@ == validated.spec_source_ids(),
            !contains_id(source_ids@, output_id),
            replacement_node_matches(
                &validated.spec_node(),
                &replacement,
                validated.spec_source_nodes(),
                source_ids@,
            ),
            nodes@.len() == survivor_count(
                graph_nodes@.take(node_index as int), source_ids@,
            ) + if replacement_inserted { 1nat } else { 0nat },
            replacement_inserted ==> exists |index: int| #![trigger nodes@[index]] {
                &&& 0 <= index < nodes@.len()
                &&& replacement_node_matches(
                    &validated.spec_node(),
                    &nodes@[index],
                    validated.spec_source_nodes(),
                    source_ids@,
                )
            },
            forall |after_index: int| #![trigger nodes@[after_index]]
                0 <= after_index < nodes@.len() ==> {
                    ||| replacement_node_matches(
                        &validated.spec_node(),
                        &nodes@[after_index],
                        validated.spec_source_nodes(),
                        source_ids@,
                    )
                    ||| exists |before_index: int| #![trigger graph_nodes@[before_index]] {
                        &&& 0 <= before_index < node_index
                        &&& surviving_node_matches(
                            &graph_nodes@[before_index],
                            &nodes@[after_index],
                            source_ids@,
                            output_id,
                        )
                    }
                },
        decreases graph_nodes.len() - node_index,
    {
        let node = &graph_nodes[node_index];
        if !replacement_inserted && output_id < node.id() {
            let inserted_replacement = replacement.clone();
            let ghost before_nodes = nodes@;
            proof {
                reveal(replacement_node_matches);
                reveal(crate::ContextNode::clone_equivalent);
                assert(replacement_node_matches(
                    &validated.spec_node(),
                    &inserted_replacement,
                    validated.spec_source_nodes(),
                    source_ids@,
                ));
            };
            nodes.push(inserted_replacement);
            replacement_inserted = true;
            proof {
                assert(nodes@ == before_nodes.push(nodes@.last()));
                assert(nodes@.len() == before_nodes.len() + 1);
                assert(exists |index: int| #![trigger nodes@[index]] {
                    &&& 0 <= index < nodes@.len()
                    &&& replacement_node_matches(
                        &validated.spec_node(),
                        &nodes@[index],
                        validated.spec_source_nodes(),
                        source_ids@,
                    )
                });
                assert forall |after_index: int| #![trigger nodes@[after_index]]
                    0 <= after_index < nodes@.len() ==> {
                        ||| replacement_node_matches(
                            &validated.spec_node(),
                            &nodes@[after_index],
                            validated.spec_source_nodes(),
                            source_ids@,
                        )
                        ||| exists |before_index: int| #![trigger graph_nodes@[before_index]] {
                            &&& 0 <= before_index < node_index
                            &&& surviving_node_matches(
                                &graph_nodes@[before_index],
                                &nodes@[after_index],
                                source_ids@,
                                output_id,
                            )
                        }
                    } by {
                        if 0 <= after_index < nodes@.len() {
                            if after_index < before_nodes.len() {
                                assert(nodes@[after_index] == before_nodes[after_index]);
                            } else {
                                assert(nodes@.len() == before_nodes.len() + 1);
                                assert(after_index == before_nodes.len());
                                assert(nodes@[after_index] == inserted_replacement);
                            }
                        }
                    }
            };
        }
        let is_source = contains(source_ids, node.id());
        if !is_source {
            let dependencies = rewrite_dependencies(
                node.dependencies(),
                source_ids,
                output_id,
            );
            let rewritten_node = node.replace_dependencies(dependencies, graph.limits())?;
            let ghost before_nodes = nodes@;
            proof {
                reveal(surviving_node_matches);
                assert(!contains_id(source_ids@, node.spec_id()));
                assert(crate::ContextNode::dependencies_replaced(
                    node, &rewritten_node, rewritten_node.spec_dependencies(),
                ));
                assert forall |id: ContextNodeId|
                    contains_id(rewritten_node.spec_dependencies(), id) ==
                        rewritten_dependency_contains(
                            node.spec_dependencies(), source_ids@, output_id, id,
                        ) by {}
                assert(surviving_node_matches(
                    node, &rewritten_node, source_ids@, output_id,
                ));
            }
            nodes.push(rewritten_node);
            proof {
                assert(nodes@ == before_nodes.push(nodes@.last()));
                assert(nodes@.len() == before_nodes.len() + 1);
                assert(exists |after_index: int| #![trigger nodes@[after_index]] {
                    &&& 0 <= after_index < nodes@.len()
                    &&& surviving_node_matches(
                        &graph_nodes@[node_index as int],
                        &nodes@[after_index],
                        source_ids@,
                        output_id,
                    )
                });
                if replacement_inserted {
                    assert(exists |index: int| #![trigger before_nodes[index]] {
                        &&& 0 <= index < before_nodes.len()
                        &&& replacement_node_matches(
                            &validated.spec_node(),
                            &before_nodes[index],
                            validated.spec_source_nodes(),
                            source_ids@,
                        )
                    });
                    let index = choose |index: int| #![trigger before_nodes[index]] {
                        &&& 0 <= index < before_nodes.len()
                        &&& replacement_node_matches(
                            &validated.spec_node(),
                            &before_nodes[index],
                            validated.spec_source_nodes(),
                            source_ids@,
                        )
                    };
                    assert(nodes@[index] == before_nodes[index]);
                    assert(exists |index: int| #![trigger nodes@[index]] {
                        &&& 0 <= index < nodes@.len()
                        &&& replacement_node_matches(
                            &validated.spec_node(),
                            &nodes@[index],
                            validated.spec_source_nodes(),
                            source_ids@,
                        )
                    });
                }
                assert forall |after_index: int| #![trigger nodes@[after_index]]
                    0 <= after_index < nodes@.len() ==> {
                        ||| replacement_node_matches(
                            &validated.spec_node(),
                            &nodes@[after_index],
                            validated.spec_source_nodes(),
                            source_ids@,
                        )
                        ||| exists |before_index: int| #![trigger graph_nodes@[before_index]] {
                            &&& 0 <= before_index < node_index + 1
                            &&& surviving_node_matches(
                                &graph_nodes@[before_index],
                                &nodes@[after_index],
                                source_ids@,
                                output_id,
                            )
                        }
                    } by {
                        if 0 <= after_index < nodes@.len() {
                            if after_index < before_nodes.len() {
                                assert(nodes@[after_index] == before_nodes[after_index]);
                            } else {
                                assert(nodes@.len() == before_nodes.len() + 1);
                                assert(after_index == before_nodes.len());
                                assert(nodes@[after_index] == rewritten_node);
                                assert(node == &graph_nodes@[node_index as int]);
                            }
                        }
                    }
            };
        }
        proof {
            assert(graph_nodes@.take(node_index as int).push(graph_nodes@[node_index as int])
                =~= graph_nodes@.take(node_index as int + 1));
            survivor_count_push(
                graph_nodes@.take(node_index as int),
                source_ids@,
                graph_nodes@[node_index as int],
            );
        }
        node_index += 1;
    }
    let ghost logical_replacement = replacement;
    if !replacement_inserted {
        let ghost before_nodes = nodes@;
        nodes.push(replacement);
        proof {
            assert(nodes@ == before_nodes.push(logical_replacement));
            assert(replacement_node_matches(
                &validated.spec_node(),
                &nodes@.last(),
                validated.spec_source_nodes(),
                source_ids@,
            ));
            assert forall |after_index: int| #![trigger nodes@[after_index]]
                0 <= after_index < nodes@.len() ==> {
                    ||| replacement_node_matches(
                        &validated.spec_node(),
                        &nodes@[after_index],
                        validated.spec_source_nodes(),
                        source_ids@,
                    )
                    ||| exists |before_index: int| #![trigger graph_nodes@[before_index]] {
                        &&& 0 <= before_index < node_index
                        &&& surviving_node_matches(
                            &graph_nodes@[before_index],
                            &nodes@[after_index],
                            source_ids@,
                            output_id,
                        )
                    }
                } by {
                    if 0 <= after_index < nodes@.len() {
                        if after_index < before_nodes.len() {
                            assert(nodes@[after_index] == before_nodes[after_index]);
                        } else {
                            assert(nodes@.len() == before_nodes.len() + 1);
                            assert(after_index == before_nodes.len());
                        }
                    }
                }
        };
    }
    proof {
        assert(node_index == graph_nodes.len());
        assert(graph_nodes@.take(node_index as int) =~= graph_nodes@);
        assert(nodes@.len() == survivor_count(graph_nodes@, source_ids@) + 1);
        assert(exists |index: int| #![trigger nodes@[index]] {
            &&& 0 <= index < nodes@.len()
            &&& replacement_node_matches(
                &validated.spec_node(),
                &nodes@[index],
                validated.spec_source_nodes(),
                source_ids@,
            )
        });
        assert forall |after_index: int| #![trigger nodes@[after_index]]
            0 <= after_index < nodes@.len() ==> {
                ||| replacement_node_matches(
                    &validated.spec_node(),
                    &nodes@[after_index],
                    validated.spec_source_nodes(),
                    source_ids@,
                )
                ||| exists |before_index: int| #![trigger graph_nodes@[before_index]] {
                    &&& 0 <= before_index < graph_nodes@.len()
                    &&& surviving_node_matches(
                        &graph_nodes@[before_index],
                        &nodes@[after_index],
                        source_ids@,
                        output_id,
                    )
                }
            } by {}
    }
    let replacement_graph = ContextGraph::new(nodes, graph.limits())?;
    proof {
        reveal(exact_replacement_graph);
        assert(replacement_graph.spec_limits() == graph.spec_limits());
        assert(replacement_graph.spec_nodes().len()
            == survivor_count(graph.spec_nodes(), validated.spec_source_ids()) + 1);
        assert(exists |index: int| #![trigger replacement_graph.spec_nodes()[index]] {
            &&& 0 <= index < replacement_graph.spec_nodes().len()
            &&& replacement_node_matches(
                &validated.spec_node(),
                &replacement_graph.spec_nodes()[index],
                validated.spec_source_nodes(),
                validated.spec_source_ids(),
            )
        });
        assert forall |after_index: int| #![trigger replacement_graph.spec_nodes()[after_index]]
            0 <= after_index < replacement_graph.spec_nodes().len() ==> {
                ||| replacement_node_matches(
                    &validated.spec_node(),
                    &replacement_graph.spec_nodes()[after_index],
                    validated.spec_source_nodes(),
                    validated.spec_source_ids(),
                )
                ||| exists |before_index: int| #![trigger graph.spec_nodes()[before_index]] {
                    &&& 0 <= before_index < graph.spec_nodes().len()
                    &&& surviving_node_matches(
                        &graph.spec_nodes()[before_index],
                        &replacement_graph.spec_nodes()[after_index],
                        validated.spec_source_ids(),
                        validated.spec_node().spec_id(),
                    )
                }
            } by {}
        assert(exact_replacement_graph(graph, validated, &replacement_graph));
    }
    Ok(replacement_graph)
}

} // verus!
