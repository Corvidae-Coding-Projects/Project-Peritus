//! Small executable planning predicates proved against their mathematical definitions.

#![allow(clippy::option_if_let_else, reason = "explicit matches keep Verus proofs branch-local")]

use vstd::prelude::*;

verus! {

/// Mathematical exact-successor relation for an absent/present CAS row.
pub closed spec fn spec_cas_successor(expected: Option<u64>, planned: u64) -> bool {
    match expected {
        None => planned == 1,
        Some(value) => value < u64::MAX && planned == value + 1,
    }
}

/// Executable exact-successor predicate reused by state and registry planning.
pub const fn cas_successor(expected: Option<u64>, planned: u64) -> (valid: bool)
    ensures valid == spec_cas_successor(expected, planned)
{
    match expected {
        None => planned == 1,
        Some(value) => match value.checked_add(1) {
            Some(successor) => successor == planned,
            None => false,
        },
    }
}

/// Mathematical aggregate sequence-extension relation, including genesis.
pub closed spec fn spec_extends_sequence(
    stored_sequence: Option<u64>,
    planned_sequence: u64,
) -> bool {
    spec_cas_successor(stored_sequence, planned_sequence)
}

/// Executable sequence-extension predicate reused by append planning.
pub const fn extends_sequence(
    stored_sequence: Option<u64>,
    planned_sequence: u64,
) -> (valid: bool)
    ensures valid == spec_extends_sequence(stored_sequence, planned_sequence)
{
    cas_successor(stored_sequence, planned_sequence)
}

/// Mathematical post-commit state observation relation.
pub closed spec fn spec_committed_state_successor(
    expected: Option<u64>,
    planned: u64,
    observed: u64,
) -> bool {
    spec_cas_successor(expected, planned) && observed == planned
}

/// Checks that an observed durable state is the exact planned CAS successor.
pub const fn committed_state_successor(
    expected: Option<u64>,
    planned: u64,
    observed: u64,
) -> (valid: bool)
    ensures valid == spec_committed_state_successor(expected, planned, observed)
{
    cas_successor(expected, planned) && observed == planned
}

/// Mathematical strictly monotonic credential-registry installation relation.
pub closed spec fn spec_registry_advance(
    stored_revision: Option<u64>,
    planned_revision: u64,
    stored_generation: Option<u64>,
    planned_generation: u64,
) -> bool {
    spec_cas_successor(stored_revision, planned_revision)
        && match stored_generation {
            None => planned_generation > 0,
            Some(generation) => planned_generation > generation,
        }
}

/// Checks exact registry-revision succession and strict generation increase.
pub const fn registry_advance(
    stored_revision: Option<u64>,
    planned_revision: u64,
    stored_generation: Option<u64>,
    planned_generation: u64,
) -> (valid: bool)
    ensures valid == spec_registry_advance(
        stored_revision,
        planned_revision,
        stored_generation,
        planned_generation,
    )
{
    cas_successor(stored_revision, planned_revision)
        && match stored_generation {
            None => planned_generation > 0,
            Some(generation) => planned_generation > generation,
        }
}

/// Mathematical authority-epoch allocation relation.
pub closed spec fn spec_authority_epoch_successor(
    stored_epoch: Option<u64>,
    planned_epoch: u64,
) -> bool {
    spec_cas_successor(stored_epoch, planned_epoch)
}

/// Checks that an allocated authority epoch is the exact durable successor.
pub const fn authority_epoch_successor(
    stored_epoch: Option<u64>,
    planned_epoch: u64,
) -> (valid: bool)
    ensures valid == spec_authority_epoch_successor(stored_epoch, planned_epoch)
{
    cas_successor(stored_epoch, planned_epoch)
}

} // verus!
