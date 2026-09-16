//! Logical relation independently checked before a selection plan escapes.

use crate::{ContextGraph, ContextNodeId, ContextPlan};
use vstd::prelude::*;

verus! {

pub open spec fn selected_references_graph(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    forall |index: int| #![trigger plan.spec_selected()[index]]
        0 <= index < plan.spec_selected().len() ==>
            graph.spec_contains_node(plan.spec_selected()[index].spec_node_id())
}

pub open spec fn selected_ids_unique(plan: &ContextPlan) -> bool {
    forall |left: int, right: int| #![trigger plan.spec_selected()[left], plan.spec_selected()[right]]
        0 <= left < right < plan.spec_selected().len() ==>
            !plan.spec_selected()[left].spec_node_id().spec_matches(
                &plan.spec_selected()[right].spec_node_id(),
            )
}

pub open spec fn omitted_references_graph(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    forall |index: int| #![trigger plan.spec_omitted()[index]]
        0 <= index < plan.spec_omitted().len() ==>
            graph.spec_contains_node(plan.spec_omitted()[index].spec_node_id())
}

pub open spec fn omitted_ids_unique(plan: &ContextPlan) -> bool {
    forall |left: int, right: int| #![trigger plan.spec_omitted()[left], plan.spec_omitted()[right]]
        0 <= left < right < plan.spec_omitted().len() ==>
            !plan.spec_omitted()[left].spec_node_id().spec_matches(
                &plan.spec_omitted()[right].spec_node_id(),
            )
}

pub open spec fn selected_and_omitted_are_disjoint(plan: &ContextPlan) -> bool {
    forall |index: int| #![trigger plan.spec_omitted()[index]]
        0 <= index < plan.spec_omitted().len() ==>
            !plan.spec_contains(plan.spec_omitted()[index].spec_node_id())
}

/// Independently stated logical meaning of a structurally exact plan.
pub open spec fn plan_exact_relation(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    &&& selected_references_graph(graph, plan)
    &&& selected_ids_unique(plan)
    &&& omitted_references_graph(graph, plan)
    &&& omitted_ids_unique(plan)
    &&& selected_and_omitted_are_disjoint(plan)
    &&& plan.spec_accounting().spec_is_bounded()
}

} // verus!
