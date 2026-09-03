//! Canonical length-delimited digests for retained knowledge inputs.

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

pub(super) fn digest(domain: &[u8], bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(u64::try_from(domain.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(domain);
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
    Sha256Digest::new(hasher.finalize().into())
}

pub(super) fn digest_pair(domain: &[u8], left: &[u8], right: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(u64::try_from(domain.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(domain);
    hasher.update(u64::try_from(left.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(left);
    hasher.update(u64::try_from(right.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(right);
    Sha256Digest::new(hasher.finalize().into())
}
