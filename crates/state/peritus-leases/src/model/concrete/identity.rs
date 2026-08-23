//! Exact nominal identity views used by executable validation refinement.

use vstd::prelude::*;

verus! {

/// Exact fixed-width identifier equality from one verified byte index.
pub open spec fn concrete_identifier_matches_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && concrete_identifier_matches_from(left, right, index + 1)
    }
}

/// Exact equality through all sixteen bytes of a nominal identifier.
pub open spec fn concrete_identifier_matches(
    left: [u8; 16],
    right: [u8; 16],
) -> bool {
    concrete_identifier_matches_from(left, right, 0)
}

/// Compares all sixteen bytes of two nominal identifier representations.
pub const fn identifier_values_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (result: bool)
    requires index <= 16,
    ensures result == concrete_identifier_matches_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}

/// Compares complete fixed-width nominal identifier representations.
pub const fn identifier_values_equal(
    left: [u8; 16],
    right: [u8; 16],
) -> (result: bool)
    ensures result == concrete_identifier_matches(left, right),
{
    identifier_values_equal_from(left, right, 0)
}

/// Opaque public wrapper for exact fieldwise lease-scope identity.
pub closed spec fn exact_lease_scope_match(
    left: crate::LeaseScope,
    right: crate::LeaseScope,
) -> bool {
    concrete_scope_matches(left, right)
}

/// Compares every nominal identifier dimension of two lease scopes.
pub const fn lease_scopes_equal(
    left: crate::LeaseScope,
    right: crate::LeaseScope,
) -> (result: bool)
    ensures result == exact_lease_scope_match(left, right),
{
    if !identifier_values_equal(
        *left.workspace.as_bytes(),
        *right.workspace.as_bytes(),
    ) {
        return false;
    }
    if !identifier_values_equal(
        *left.resource.as_bytes(),
        *right.resource.as_bytes(),
    ) {
        return false;
    }
    identifier_values_equal(
        *left.environment.as_bytes(),
        *right.environment.as_bytes(),
    )
}

/// Symmetry of exact identifier equality from a verified byte index.
pub(crate) proof fn identifier_matches_symmetric_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
)
    requires
        index <= 16,
        concrete_identifier_matches_from(left, right, index),
    ensures concrete_identifier_matches_from(right, left, index),
    decreases 16 - index,
{
    if index < 16 {
        identifier_matches_symmetric_from(left, right, index + 1);
    }
}

/// Transitivity of exact identifier equality from a verified byte index.
pub(crate) proof fn identifier_matches_transitive_from(
    left: [u8; 16],
    middle: [u8; 16],
    right: [u8; 16],
    index: nat,
)
    requires
        index <= 16,
        concrete_identifier_matches_from(left, middle, index),
        concrete_identifier_matches_from(middle, right, index),
    ensures concrete_identifier_matches_from(left, right, index),
    decreases 16 - index,
{
    if index < 16 {
        identifier_matches_transitive_from(left, middle, right, index + 1);
    }
}

/// Two identifiers that exactly match one common identity match each other.
pub(crate) proof fn identifiers_matching_common_identity_match(
    first: [u8; 16],
    common: [u8; 16],
    second: [u8; 16],
)
    requires
        concrete_identifier_matches(first, common),
        concrete_identifier_matches(second, common),
    ensures concrete_identifier_matches(first, second),
{
    identifier_matches_symmetric_from(second, common, 0);
    identifier_matches_transitive_from(first, common, second, 0);
}

/// Exact holders that both match one current holder match one another.
pub(crate) proof fn current_holders_match(
    first: crate::LeaseHolder,
    current: crate::LeaseHolder,
    second: crate::LeaseHolder,
)
    requires
        concrete_holder_matches(first, current),
        concrete_holder_matches(second, current),
    ensures concrete_holder_matches(first, second),
{
    identifiers_matching_common_identity_match(
        first.actor_id.spec_bytes(),
        current.actor_id.spec_bytes(),
        second.actor_id.spec_bytes(),
    );
    identifiers_matching_common_identity_match(
        first.session_id.spec_bytes(),
        current.session_id.spec_bytes(),
        second.session_id.spec_bytes(),
    );
}

/// Exact lease-scope identity through closed nominal-identifier views.
pub(crate) open spec fn concrete_scope_matches(
    left: crate::LeaseScope,
    right: crate::LeaseScope,
) -> bool {
    concrete_identifier_matches(
        left.workspace.spec_bytes(),
        right.workspace.spec_bytes(),
    ) && concrete_identifier_matches(
        left.resource.spec_bytes(),
        right.resource.spec_bytes(),
    ) && concrete_identifier_matches(
        left.environment.spec_bytes(),
        right.environment.spec_bytes(),
    )
}

/// Exact actor/session holder identity through closed nominal-identifier views.
pub(crate) open spec fn concrete_holder_matches(
    left: crate::LeaseHolder,
    right: crate::LeaseHolder,
) -> bool {
    concrete_identifier_matches(left.actor_id.spec_bytes(), right.actor_id.spec_bytes())
        && concrete_identifier_matches(left.session_id.spec_bytes(), right.session_id.spec_bytes())
}

/// Exact authority instant identity through its epoch/tick view.
pub(crate) open spec fn concrete_instant_matches(
    left: peritus_policy::AuthorityInstant,
    right: peritus_policy::AuthorityInstant,
) -> bool {
    left.spec_epoch() == right.spec_epoch()
        && left.spec_tick_millis() == right.spec_tick_millis()
}

/// Exact fieldwise identity for an unprivileged claim.
pub(crate) open spec fn concrete_claim_matches(
    left: crate::LeaseClaim,
    right: crate::LeaseClaim,
) -> bool {
    concrete_scope_matches(left.scope, right.scope)
        && concrete_holder_matches(left.holder, right.holder)
        && left.generation.spec_value() == right.generation.spec_value()
        && left.claim_version.spec_value() == right.claim_version.spec_value()
        && concrete_instant_matches(left.issued_at, right.issued_at)
        && concrete_instant_matches(left.expires_at, right.expires_at)
}

} // verus!
