//! Approval replay, binding, terminality, and reachability proofs.

use vstd::prelude::*;

verus! {

mod binding;
mod refinement;
mod replay;

pub(crate) use binding::consume_once_preserves_digest;
pub(crate) use refinement::{accepted_reducer_refines, rejected_reducer_preserves};
pub(crate) use replay::exact_replay_is_idempotent;
} // verus!
