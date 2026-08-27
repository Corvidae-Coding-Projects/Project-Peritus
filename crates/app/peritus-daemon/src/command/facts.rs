//! Domain-separated digests for retained application command result facts.

use peritus_journal::CommittedBatch;
use peritus_types::{CommandId, Sha256Digest};
use sha2::{Digest, Sha256};

pub(crate) fn committed_result_digest(batch: &CommittedBatch) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/application-command-result/v1\0");
    hasher.update(batch.command_id().as_bytes());
    hasher.update(batch.request_digest().as_bytes());
    hasher.update(batch.first_position().to_be_bytes());
    hasher.update(batch.last_position().to_be_bytes());
    hasher.update(batch.batch_hash().as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}

pub(crate) fn rejection_result_digest(
    command_id: CommandId,
    request_digest: Sha256Digest,
    code: &str,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/application-command-rejection/v1\0");
    hasher.update(command_id.as_bytes());
    hasher.update(request_digest.as_bytes());
    hasher.update(code.as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}
