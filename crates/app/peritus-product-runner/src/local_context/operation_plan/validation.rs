//! Validate the entire inert dependency graph before admitting its first operation.

use super::{
    contract::{MAX_NODES, Node},
    error,
};
use peritus_agent::DeveloperLoopError;
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn nodes(nodes: &[Node]) -> Result<(), DeveloperLoopError> {
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return Err(error("plan page requires 1 through 64 nodes"));
    }
    let mut prior = BTreeSet::new();
    for node in nodes {
        if node.id.is_empty()
            || node.id.len() > 64
            || !node.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            || prior.contains(&node.id)
        {
            return Err(error("plan node identities must be unique short ASCII labels"));
        }
        let dependencies = node.depends_on.iter().collect::<BTreeSet<_>>();
        if dependencies.len() != node.depends_on.len() || !dependencies.is_subset(&prior) {
            return Err(error("plan dependencies must name distinct earlier nodes"));
        }
        let arguments =
            node.arguments.as_object().ok_or_else(|| error("plan arguments must be an object"))?;
        let mut fields = BTreeSet::new();
        for reference in &node.result_refs {
            if arguments.contains_key(&reference.argument) {
                return Err(error(
                    "omit a result_refs argument from arguments entirely, including empty or null placeholders; a literal and a reference cannot supply the same field",
                ));
            }
            if !dependencies.contains(&reference.node)
                || !fields.insert(reference.argument.clone())
                || !pointer(&reference.pointer)
            {
                return Err(error(
                    "plan result reference is ambiguous or not an earlier dependency",
                ));
            }
        }
        crate::developer_tools::validate_inspection_plan_arguments(
            node.tool.name(),
            &node.arguments,
            &fields.into_iter().collect::<Vec<_>>(),
        )?;
        prior.insert(&node.id);
    }
    Ok(())
}

fn pointer(pointer: &str) -> bool {
    if pointer.len() > 4096 || (!pointer.is_empty() && !pointer.starts_with('/')) {
        return false;
    }
    let mut bytes = pointer.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return false;
        }
    }
    true
}

pub(super) fn resolve(
    node: &Node,
    outputs: &[(String, Value, bool)],
) -> Result<Value, DeveloperLoopError> {
    for dependency in &node.depends_on {
        if outputs.iter().find(|(id, _, _)| id == dependency).is_none_or(|(_, _, failed)| *failed) {
            return Err(error("plan dependency failed or is unavailable; model judgment required"));
        }
    }
    let mut arguments = node.arguments.clone();
    for reference in &node.result_refs {
        let output = outputs
            .iter()
            .find(|(id, _, _)| id == &reference.node)
            .and_then(|(_, output, _)| output.pointer(&reference.pointer))
            .ok_or_else(|| error("plan result reference unavailable; model judgment required"))?;
        arguments
            .as_object_mut()
            .ok_or_else(|| error("invalid plan arguments"))?
            .insert(reference.argument.clone(), output.clone());
    }
    crate::developer_tools::validate_inspection_plan_arguments(node.tool.name(), &arguments, &[])?;
    Ok(arguments)
}
