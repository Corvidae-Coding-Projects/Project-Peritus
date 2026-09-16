//! Production validation and exact lookup for canonical context graphs.

#[cfg(verus_only)]
use super::model;
use crate::{ContextError, ContextErrorKind, ContextNode, ContextNodeId};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(super) fn validate_node_order(
    nodes: &[ContextNode],
) -> (result: Result<(), ContextError>)
    requires nodes@.len() > 0,
    ensures
        result.is_ok() == model::nodes_canonical(nodes@),
        match result {
            Ok(()) => true,
            Err(error) => match model::first_node_order_error(nodes@, 1) {
                Some((kind, id)) => error.spec_is_node(kind, id),
                None => false,
            },
        },
{
    let mut index = 1;
    while index < nodes.len()
        invariant
            1 <= index <= nodes.len(),
            model::first_node_order_error(nodes@, 1)
                == model::first_node_order_error(nodes@, index as nat),
        decreases nodes.len() - index,
    {
        match nodes[index - 1].id().canonical_order(&nodes[index].id()) {
            Ordering::Equal => {
                return Err(ContextError::node(ContextErrorKind::DuplicateValue, nodes[index].id()));
            }
            Ordering::Greater => {
                return Err(ContextError::node(
                    ContextErrorKind::NonCanonicalOrder,
                    nodes[index].id(),
                ));
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    Ok(())
}

pub(super) fn find_node_index(
    nodes: &[ContextNode],
    id: ContextNodeId,
) -> (result: Option<usize>)
    ensures
        model::node_index_result(nodes@, id, result),
        match result {
            Some(index) => index < nodes.len()
                && nodes@[index as int].spec_id().spec_matches(&id),
            None => true,
        },
{
    let mut index = 0;
    while index < nodes.len()
        invariant
            index <= nodes.len(),
            model::first_node_index_from(nodes@, id, 0)
                == model::first_node_index_from(nodes@, id, index as nat),
        decreases nodes.len() - index,
    {
        if nodes[index].id().matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn validate_node_dependencies(
    nodes: &[ContextNode],
    owner: ContextNodeId,
    dependencies: &[ContextNodeId],
) -> (result: Result<(), ContextError>)
    ensures
        result.is_ok()
            == model::first_missing_for_node(nodes@, owner, dependencies@, 0).is_none(),
        match result {
            Ok(()) => true,
            Err(error) => match model::first_missing_for_node(nodes@, owner, dependencies@, 0) {
                Some((expected_owner, missing)) => error.spec_is_nodes(
                    ContextErrorKind::MissingDependency,
                    expected_owner,
                    missing,
                ),
                None => false,
            },
        },
{
    let mut index = 0;
    while index < dependencies.len()
        invariant
            index <= dependencies.len(),
            model::first_missing_for_node(nodes@, owner, dependencies@, 0)
                == model::first_missing_for_node(nodes@, owner, dependencies@, index as nat),
        decreases dependencies.len() - index,
    {
        if find_node_index(nodes, dependencies[index]).is_none() {
            return Err(ContextError::nodes(
                ContextErrorKind::MissingDependency,
                owner,
                dependencies[index],
            ));
        }
        index += 1;
    }
    Ok(())
}

pub(super) fn validate_dependencies(
    nodes: &[ContextNode],
) -> (result: Result<(), ContextError>)
    ensures
        result.is_ok() == model::dependencies_exist(nodes@),
        match result {
            Ok(()) => true,
            Err(error) => match model::first_missing_dependency(nodes@, 0) {
                Some((owner, missing)) => error.spec_is_nodes(
                    ContextErrorKind::MissingDependency,
                    owner,
                    missing,
                ),
                None => false,
            },
        },
{
    let mut index = 0;
    while index < nodes.len()
        invariant
            index <= nodes.len(),
            model::first_missing_dependency(nodes@, 0)
                == model::first_missing_dependency(nodes@, index as nat),
        decreases nodes.len() - index,
    {
        validate_node_dependencies(nodes, nodes[index].id(), nodes[index].dependencies())?;
        index += 1;
    }
    Ok(())
}

} // verus!
