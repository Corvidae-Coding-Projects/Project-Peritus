//! Deterministic command-idempotency decisions.

use peritus_types::Sha256Digest;

#[doc = " Effect-free decision for one command identity and request digest."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandDecision {
    #[doc = " The identity has not been durably recorded."]
    New,
    #[doc = " The identity is bound to this exact request digest."]
    Replay,
    #[doc = " The identity is already bound to different request bytes."]
    Conflict,
}
#[doc = " Decides whether a command is new, an exact replay, or a conflicting reuse."]
#[must_use]
pub const fn decide_command(
    stored: Option<Sha256Digest>,
    requested: Sha256Digest,
) -> CommandDecision {
    match stored {
        None => CommandDecision::New,
        Some(digest) if digests_match(digest, requested) => CommandDecision::Replay,
        Some(_) => CommandDecision::Conflict,
    }
}
const fn digests_match(left: Sha256Digest, right: Sha256Digest) -> bool {
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 32 {
        if left_bytes[index] != right_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}
