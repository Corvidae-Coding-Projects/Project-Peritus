//! Small executable planning predicates proved against their mathematical definitions.

#![allow(clippy::option_if_let_else, reason = "explicit matches keep Verus proofs branch-local")]

#[doc = " Executable exact-successor predicate reused by state and registry planning."]
pub const fn cas_successor(expected: Option<u64>, planned: u64) -> bool {
    match expected {
        None => planned == 1,
        Some(value) => match value.checked_add(1) {
            Some(successor) => successor == planned,
            None => false,
        },
    }
}
#[doc = " Executable sequence-extension predicate reused by append planning."]
pub const fn extends_sequence(stored_sequence: Option<u64>, planned_sequence: u64) -> bool {
    cas_successor(stored_sequence, planned_sequence)
}
#[doc = " Checks that an observed durable state is the exact planned CAS successor."]
pub const fn committed_state_successor(expected: Option<u64>, planned: u64, observed: u64) -> bool {
    cas_successor(expected, planned) && observed == planned
}
#[doc = " Checks exact registry-revision succession and strict generation increase."]
pub const fn registry_advance(
    stored_revision: Option<u64>,
    planned_revision: u64,
    stored_generation: Option<u64>,
    planned_generation: u64,
) -> bool {
    cas_successor(stored_revision, planned_revision)
        && match stored_generation {
            None => planned_generation > 0,
            Some(generation) => planned_generation > generation,
        }
}
#[doc = " Checks that an allocated authority epoch is the exact durable successor."]
pub const fn authority_epoch_successor(stored_epoch: Option<u64>, planned_epoch: u64) -> bool {
    cas_successor(stored_epoch, planned_epoch)
}
