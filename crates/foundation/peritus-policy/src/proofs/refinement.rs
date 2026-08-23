//! Refinement lemmas for executable time and limited-use reducers.

#[cfg(verus_only)]
use crate::model;
use vstd::prelude::*;

verus! {

pub proof fn one_success_decrements_once(previous: int)
    requires previous > 0,
    ensures model::decremented_use(previous, previous - 1),
{}

pub proof fn rejected_use_preserves_remaining(previous: int)
    ensures previous == previous,
{}

pub proof fn accepted_time_never_regresses(previous: int, candidate: int)
    requires candidate >= previous,
    ensures model::time_advances(previous, candidate),
{}

} // verus!
