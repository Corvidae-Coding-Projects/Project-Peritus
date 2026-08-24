//! Digest-bound native helper protocol records.

use peritus_types::Sha256Digest;

/// Computes the helper-ready record for an exact manifest and admitted preparation.
#[must_use]
pub fn native_ready_record() -> Sha256Digest {
    peritus_codec::sha256(b"peritus-native-ready-v1")
}

/// Computes the helper-activation record for an exact manifest and admitted preparation.
#[must_use]
pub fn native_activation_record(
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Sha256Digest {
    protocol_digest(b"peritus-native-activated-v1", manifest_digest, preparation_digest)
}

/// Computes the helper record proving that the restricted target was successfully resumed.
#[must_use]
pub fn native_target_started_record(
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Sha256Digest {
    protocol_digest(b"peritus-native-target-started-v1", manifest_digest, preparation_digest)
}

/// Computes the close-on-exec helper record proving target creation failed before replacement.
#[must_use]
pub fn native_target_exec_failed_record(
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Sha256Digest {
    protocol_digest(b"peritus-native-target-exec-failed-v1", manifest_digest, preparation_digest)
}

fn protocol_digest(
    domain: &[u8],
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(domain.len() + Sha256Digest::LENGTH * 2);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(manifest_digest.as_bytes());
    bytes.extend_from_slice(preparation_digest.as_bytes());
    peritus_codec::sha256(&bytes)
}
