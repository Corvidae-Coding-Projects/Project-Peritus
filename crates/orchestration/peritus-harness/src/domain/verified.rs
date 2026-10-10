//! Executable invariants and Verus proof roots for the pure E1 domain.

use crate::domain::{CheckedHarnessGraph, HarnessHistory, HarnessRevision, RevisionDigest};

/// Returns whether component identities are unique in the checked graph.
#[must_use]
pub fn component_ids_are_unique(graph: &CheckedHarnessGraph) -> bool {
    graph.declarations().iter().enumerate().all(|(index, declaration)| {
        graph.declarations()[index + 1..].iter().all(|other| declaration.id() != other.id())
    })
}

/// Returns whether every resolved dependency is present and compatible.
#[must_use]
pub fn dependencies_are_resolved(graph: &CheckedHarnessGraph) -> bool {
    graph.declarations().iter().all(|declaration| {
        declaration.dependencies().iter().all(|requirement| {
            graph.declaration(requirement.component_id()).is_some_and(|dependency| {
                dependency.kind() == requirement.required_kind()
                    && requirement.compatible_schema().contains(dependency.schema_version())
                    && requirement
                        .exact_content_digest()
                        .is_none_or(|digest| digest == dependency.content_digest())
            })
        })
    })
}

/// Returns whether topological order covers every component exactly once dependency-first.
#[must_use]
pub fn topological_order_is_complete(graph: &CheckedHarnessGraph) -> bool {
    if graph.topological_order().len() != graph.declarations().len() {
        return false;
    }
    graph.topological_order().iter().enumerate().all(|(index, id)| {
        graph.declaration(id).is_some_and(|declaration| {
            graph.topological_order().iter().filter(|candidate| *candidate == id).count() == 1
                && declaration.dependencies().iter().all(|dependency| {
                    graph.topological_order()[..index].contains(dependency.component_id())
                })
        })
    })
}

/// Returns whether every declaration and dependency closure remains under compiled authority.
#[must_use]
pub fn authority_is_non_widening(graph: &CheckedHarnessGraph) -> bool {
    let mut closures = vec![crate::domain::AuthoritySet::empty(); graph.declarations().len()];
    for id in graph.topological_order() {
        let Some(index) = graph.declarations().iter().position(|item| item.id() == id) else {
            return false;
        };
        let declaration = &graph.declarations()[index];
        let mut closure = declaration.declared_authority();
        for dependency in declaration.dependencies() {
            let Some(dependency_index) =
                graph.declarations().iter().position(|item| item.id() == dependency.component_id())
            else {
                return false;
            };
            closure = closure.union(closures[dependency_index]);
        }
        if !closure.is_subset_of(declaration.kind().authority_ceiling()) {
            return false;
        }
        closures[index] = closure;
    }
    true
}

/// Returns whether a successor preserves the complete protected inventory.
#[must_use]
pub fn protected_assets_are_invariant(
    predecessor: &HarnessRevision,
    successor: &HarnessRevision,
) -> bool {
    successor.is_direct_successor_of(predecessor)
        && predecessor.graph().protected_assets() == successor.graph().protected_assets()
}

/// Returns whether a graph digest binds its complete canonical graph bytes.
#[must_use]
pub fn graph_digest_is_bound(graph: &CheckedHarnessGraph) -> bool {
    graph.graph_digest().digest() == peritus_codec::sha256(&graph.canonical_bytes())
}

/// Returns whether `after` has exactly the immutable prefix represented by `before`.
#[must_use]
pub fn history_is_append_only(before: &HarnessHistory, after: &HarnessHistory) -> bool {
    before.revisions().len() <= after.revisions().len()
        && before.revisions().iter().zip(after.revisions()).all(|(left, right)| left == right)
}

/// Returns whether a rollback target is a strict retained ancestor of its source.
#[must_use]
pub fn rollback_is_ancestor(
    history: &HarnessHistory,
    source: RevisionDigest,
    target: RevisionDigest,
) -> bool {
    history.validate_rollback(source, target).is_ok()
}
