//! Executable invariants and Verus proof roots for the pure E1 domain.

use vstd::prelude::*;

use crate::domain::{CheckedHarnessGraph, HarnessHistory, HarnessRevision, RevisionDigest};

verus! {

/// Mathematical non-widening predicate for a bit-set authority declaration.
pub open spec fn authority_within_ceiling(authority_bits: u64, ceiling_bits: u64) -> bool {
    authority_bits & ceiling_bits == authority_bits
}

/// Mathematical inclusive compatibility predicate.
pub open spec fn schema_is_compatible(minimum: int, maximum: int, observed: int) -> bool {
    0 < minimum && minimum <= observed && observed <= maximum
}

/// Mathematical dependency-first ordering predicate for one edge.
pub open spec fn dependency_precedes(dependency_index: int, depender_index: int) -> bool {
    0 <= dependency_index && dependency_index < depender_index
}

/// Mathematical full-digest binding predicate.
pub open spec fn digest_is_bound(expected: int, observed: int, claimed: bool) -> bool {
    !claimed || expected == observed
}

/// Mathematical protected-invariance predicate.
pub open spec fn protected_assets_unchanged(before: int, after: int) -> bool {
    before == after
}

/// Mathematical append-only length predicate.
pub open spec fn append_only_length(before: int, after: int) -> bool {
    0 <= before && before <= after
}

/// Mathematical strict-ancestor progress predicate.
pub open spec fn ancestor_number_precedes(target: int, source: int) -> bool {
    0 < target && target < source
}

/// Proves that an authority ceiling contains itself.
pub proof fn authority_reflexive(bits: u64)
    ensures authority_within_ceiling(bits, bits)
{
    assert(bits & bits == bits) by(bit_vector);
}

/// Proves a schema version is compatible with its singleton interval.
pub proof fn singleton_schema_compatible(version: int)
    requires 0 < version
    ensures schema_is_compatible(version, version, version)
{
}

/// Proves any nonnegative earlier topological position is legal.
pub proof fn earlier_dependency_is_legal(dependency_index: int, depender_index: int)
    requires 0 <= dependency_index, dependency_index < depender_index
    ensures dependency_precedes(dependency_index, depender_index)
{
}

/// Proves reflexive canonical digest binding.
pub proof fn digest_binding_reflexive(digest: int)
    ensures digest_is_bound(digest, digest, true)
{
}

/// Proves reflexive protected-asset invariance.
pub proof fn protected_invariance_reflexive(inventory: int)
    ensures protected_assets_unchanged(inventory, inventory)
{
}

/// Proves one append advances rather than shrinks history length.
pub proof fn one_append_is_monotonic(length: int)
    requires 0 <= length
    ensures append_only_length(length, length + 1)
{
}

/// Proves a positive direct predecessor has a smaller logical number.
pub proof fn direct_predecessor_is_ancestor(number: int)
    requires 1 < number
    ensures ancestor_number_precedes(number - 1, number)
{
}

} // verus!

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
