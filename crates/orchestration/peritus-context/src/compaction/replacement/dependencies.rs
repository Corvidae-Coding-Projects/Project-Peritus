//! Dependency collection and deterministic rewrite helpers.

use super::super::ValidatedCompaction;

use crate::ContextNodeId;

#[allow(
    clippy::too_many_lines,
    reason = "nested finite traversals retain the source and dependency proof invariants together"
)]
pub(super) fn external_dependencies(
    validated: &ValidatedCompaction,
    source_ids: &[ContextNodeId],
) -> Vec<ContextNodeId> {
    let mut dependencies = Vec::new();
    let mut source_index = 0;
    while source_index < validated.sources.len() {
        let source = &validated.sources[source_index];
        let source_dependencies = source.node.dependencies();
        let mut dependency_index = 0;
        while dependency_index < source_dependencies.len() {
            let dependency = source_dependencies[dependency_index];
            let is_source = contains(source_ids, dependency);
            if !is_source {
                insert_canonical(&mut dependencies, dependency);
            }
            dependency_index += 1;
        }
        source_index += 1;
    }
    dependencies
}
pub(super) fn rewrite_dependencies(
    dependencies: &[ContextNodeId],
    source_ids: &[ContextNodeId],
    output_id: ContextNodeId,
) -> Vec<ContextNodeId> {
    let mut rewritten = Vec::with_capacity(dependencies.len());
    let mut dependency_index = 0;
    while dependency_index < dependencies.len() {
        let original = dependencies[dependency_index];
        let is_source = contains(source_ids, original);
        let dependency = if is_source { output_id } else { original };
        insert_canonical(&mut rewritten, dependency);
        dependency_index += 1;
    }
    rewritten
}
#[allow(
    clippy::branches_sharing_code,
    reason = "each insertion branch establishes a distinct membership proof"
)]
pub(super) fn insert_canonical(values: &mut Vec<ContextNodeId>, value: ContextNodeId) {
    let mut position = 0;
    while position < values.len()
        && values[position].canonical_order(&value) == core::cmp::Ordering::Less
    {
        position += 1;
    }
    if position == values.len() || !values[position].matches(&value) {
        values.insert(position, value);
    } else {
    }
}
pub(super) fn contains(values: &[ContextNodeId], target: ContextNodeId) -> bool {
    let mut index = 0;
    while index < values.len() {
        if values[index].matches(&target) {
            return true;
        }
        index += 1;
    }
    false
}
