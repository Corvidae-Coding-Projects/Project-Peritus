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
pub const fn decide_command(
    stored: Option<Sha256Digest>,
    requested: Sha256Digest,
) -> (decision: CommandDecision)
    ensures
        decision == match stored {
            None => CommandDecision::New,
            Some(digest) => if digest.spec_bytes() == requested.spec_bytes() {
                CommandDecision::Replay
            } else {
                CommandDecision::Conflict
            },
        },
{
    match stored {
        None => CommandDecision::New,
        Some(digest) if digests_match(digest, requested) => CommandDecision::Replay,
        Some(_) => CommandDecision::Conflict,
    }
}

const fn digests_match(left: Sha256Digest, right: Sha256Digest) -> (equal: bool)
    ensures equal == (left.spec_bytes() == right.spec_bytes()),
{
    let left_bytes = left.into_bytes();
    let right_bytes = right.into_bytes();
    let mut index = 0;
    while index < 32
        invariant
            index <= 32,
            left_bytes == left.spec_bytes(),
            right_bytes == right.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==> left_bytes[prior] == right_bytes[prior],
        decreases 32 - index,
    {
        if left_bytes[index] != right_bytes[index] {
            assert(left.spec_bytes()[index as int] != right.spec_bytes()[index as int]);
            return false;
        }
        index += 1;
    }
    assert(left_bytes =~= right_bytes);
    true
}

} // verus!

#[cfg(test)]
mod tests {
    use super::{CommandDecision, decide_command};
    use peritus_types::Sha256Digest;

    #[test]
    fn replay_requires_all_request_digest_bytes_to_match() {
        let bytes = [0xA5; 32];
        let retained = Sha256Digest::new(bytes);
        assert_eq!(decide_command(None, retained), CommandDecision::New);
        assert_eq!(decide_command(Some(retained), retained), CommandDecision::Replay);
        for index in 0..bytes.len() {
            let mut changed = bytes;
            changed[index] ^= 1;
            assert_eq!(
                decide_command(Some(retained), Sha256Digest::new(changed)),
                CommandDecision::Conflict,
                "changed digest byte {index}",
            );
        }
    }
}
