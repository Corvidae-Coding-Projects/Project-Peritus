//! Proofs for exact permission pairs and preserved complete scope (`INV-008`).

use vstd::prelude::*;

verus! {

pub open spec fn exact_pair_authorized(
    pairs: Set<(int, int)>,
    resource: int,
    operation: int,
) -> bool {
    pairs.contains((resource, operation))
}

pub proof fn permission_membership_is_one_exact_pair(
    pairs: Set<(int, int)>,
    resource: int,
    operation: int,
)
    ensures
        exact_pair_authorized(pairs, resource, operation)
            == pairs.contains((resource, operation)),
{}

pub proof fn independent_atoms_do_not_imply_cartesian_authority(
    pairs: Set<(int, int)>,
    first_resource: int,
    first_operation: int,
    second_resource: int,
    second_operation: int,
)
    requires
        exact_pair_authorized(pairs, first_resource, first_operation),
        exact_pair_authorized(pairs, second_resource, second_operation),
        !pairs.contains((first_resource, second_operation)),
    ensures
        !exact_pair_authorized(pairs, first_resource, second_operation),
{}

pub open spec fn scope_preserved(
    prior_actor: int,
    successor_actor: int,
    prior_environment: int,
    successor_environment: int,
    prior_revision: int,
    successor_revision: int,
) -> bool {
    prior_actor == successor_actor
        && prior_environment == successor_environment
        && prior_revision == successor_revision
}

pub proof fn identical_scope_dimensions_are_preserved(
    actor: int,
    environment: int,
    revision: int,
)
    ensures scope_preserved(actor, actor, environment, environment, revision, revision),
{}

} // verus!
