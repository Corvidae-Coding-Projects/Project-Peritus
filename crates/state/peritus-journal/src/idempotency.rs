//! Deterministic command-idempotency decisions.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Effect-free decision for one command identity and request digest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandDecision {
    /// The identity has not been durably recorded.
    New,
    /// The identity is bound to this exact request digest.
    Replay,
    /// The identity is already bound to different request bytes.
    Conflict,
}

/// Decides whether a command is new, an exact replay, or a conflicting reuse.
#[must_use]
pub fn decide_command(
    stored: Option<Sha256Digest>,
    requested: Sha256Digest,
) -> CommandDecision {
    match stored {
        None => CommandDecision::New,
        Some(digest) if digest == requested => CommandDecision::Replay,
        Some(_) => CommandDecision::Conflict,
    }
}

} // verus!
