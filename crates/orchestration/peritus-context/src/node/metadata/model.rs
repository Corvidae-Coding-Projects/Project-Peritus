//! Recursive specification of metadata admission and failure priority.

use crate::{AuthorityClass, ContentKind};
#[cfg(verus_only)]
use crate::{ContextError, ContextErrorKind, ContextNodeId};
#[cfg(verus_only)]
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Exact first dependency error at or after `index`.
pub open spec fn first_dependency_error(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    index: nat,
) -> Option<(ContextErrorKind, ContextNodeId)>
    decreases dependencies.len() - index,
{
    if index >= dependencies.len() {
        None
    } else if dependencies[index as int].spec_matches(&id) {
        Some((ContextErrorKind::SelfDependency, id))
    } else if index > 0
        && dependencies[index as int - 1].spec_order(&dependencies[index as int])
            == Ordering::Equal
    {
        Some((ContextErrorKind::DuplicateValue, dependencies[index as int]))
    } else if index > 0
        && dependencies[index as int - 1].spec_order(&dependencies[index as int])
            == Ordering::Greater
    {
        Some((ContextErrorKind::NonCanonicalOrder, dependencies[index as int]))
    } else {
        first_dependency_error(id, dependencies, index + 1)
    }
}

/// Whether every dependency is non-self and in strict canonical order.
pub open spec fn dependencies_valid(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
) -> bool {
    first_dependency_error(id, dependencies, 0).is_none()
}

/// Exact structured error produced by dependency validation.
pub open spec fn dependency_error(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    error: ContextError,
) -> bool {
    match first_dependency_error(id, dependencies, 0) {
        Some((kind, related)) => error.spec_is_nodes(kind, id, related),
        None => false,
    }
}

/// Exact content-kind and authority compatibility relation.
pub open spec fn kind_matches_authority(
    kind: ContentKind,
    authority: AuthorityClass,
) -> bool {
    match kind {
        ContentKind::SystemPolicy => authority == AuthorityClass::SystemPolicy,
        ContentKind::ApplicationPolicy => authority == AuthorityClass::ApplicationPolicy,
        ContentKind::ImmutableSpecification => authority == AuthorityClass::AcceptanceSpecification,
        ContentKind::ActiveUserInstruction => authority == AuthorityClass::UserInstruction,
        ContentKind::CapabilityFact => authority != AuthorityClass::NonAuthoritative,
        _ => authority == AuthorityClass::NonAuthoritative,
    }
}

pub(super) const fn kind_matches_authority_exec(
    kind: ContentKind,
    authority: AuthorityClass,
) -> (matches: bool)
    ensures matches == kind_matches_authority(kind, authority),
{
    match kind {
        ContentKind::SystemPolicy => matches!(authority, AuthorityClass::SystemPolicy),
        ContentKind::ApplicationPolicy => matches!(authority, AuthorityClass::ApplicationPolicy),
        ContentKind::ImmutableSpecification => {
            matches!(authority, AuthorityClass::AcceptanceSpecification)
        }
        ContentKind::ActiveUserInstruction => matches!(authority, AuthorityClass::UserInstruction),
        ContentKind::CapabilityFact => !matches!(authority, AuthorityClass::NonAuthoritative),
        _ => matches!(authority, AuthorityClass::NonAuthoritative),
    }
}

proof fn dependency_error_suffix_is_empty(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    index: nat,
)
    requires
        first_dependency_error(id, dependencies, 0).is_none(),
        index <= dependencies.len(),
    ensures first_dependency_error(id, dependencies, index).is_none(),
    decreases index,
{
    if index > 0 {
        dependency_error_suffix_is_empty(id, dependencies, (index - 1) as nat);
        reveal(first_dependency_error);
    }
}

proof fn valid_dependency_adjacent(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    index: nat,
)
    requires
        dependencies_valid(id, dependencies),
        1 <= index < dependencies.len(),
    ensures dependencies[index as int - 1].spec_order(
        &dependencies[index as int],
    ) == Ordering::Less,
{
    reveal(dependencies_valid);
    dependency_error_suffix_is_empty(id, dependencies, index);
    reveal(first_dependency_error);
    match dependencies[index as int - 1].spec_order(&dependencies[index as int]) {
        Ordering::Less => {}
        Ordering::Equal => { assert(false); }
        Ordering::Greater => { assert(false); }
    }
}

proof fn valid_dependency_pair(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    left: nat,
    right: nat,
)
    requires
        dependencies_valid(id, dependencies),
        left < right < dependencies.len(),
    ensures dependencies[left as int].spec_order(
        &dependencies[right as int],
    ) == Ordering::Less,
    decreases right - left,
{
    valid_dependency_adjacent(id, dependencies, right);
    if left + 1 < right {
        valid_dependency_pair(id, dependencies, left, (right - 1) as nat);
        ContextNodeId::order_transitive(
            &dependencies[left as int],
            &dependencies[right as int - 1],
            &dependencies[right as int],
        );
    }
}

pub(super) proof fn valid_dependency_match_is_unique(
    id: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    left: nat,
    right: nat,
)
    requires
        dependencies_valid(id, dependencies),
        left < dependencies.len(),
        right < dependencies.len(),
        dependencies[left as int].spec_matches(&dependencies[right as int]),
    ensures left == right,
{
    ContextNodeId::matches_implies_equal(
        &dependencies[left as int],
        &dependencies[right as int],
    );
    if left < right {
        valid_dependency_pair(id, dependencies, left, right);
        ContextNodeId::order_reflexive(&dependencies[left as int]);
        assert(false);
    } else if right < left {
        valid_dependency_pair(id, dependencies, right, left);
        ContextNodeId::order_reflexive(&dependencies[left as int]);
        assert(false);
    }
}

} // verus!
