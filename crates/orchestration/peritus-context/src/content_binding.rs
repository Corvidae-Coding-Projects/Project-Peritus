//! Executable SHA-256 boundary shared by ordinary and Verus-configured consumers.
//!
//! The real codec implementation remains outside the verified core; this module does not
//! assume a hash specification or claim to prove SHA-256. Bounds construction stays in `content`.

use crate::{ContextContent, ContextError, ContextErrorKind, ContextLimits};
use peritus_codec::sha256;
use peritus_types::Sha256Digest;

/// Validates content bounds and its exact SHA-256 digest.
///
/// SHA-256 is the crate's audited H-class boundary; all bounds and metadata validation remain in
/// Verus code. This is the only public way to construct [`ContextContent`].
///
/// # Errors
///
/// Returns a typed error for empty, oversized, or digest-mismatched content.
pub fn bind_context_content(
    bytes: Vec<u8>,
    digest: Sha256Digest,
    limits: ContextLimits,
) -> Result<ContextContent, ContextError> {
    if sha256(bytes.as_slice()) != digest {
        return Err(ContextError::plain(ContextErrorKind::DigestMismatch));
    }
    ContextContent::from_digest_checked(bytes, digest, limits)
}
