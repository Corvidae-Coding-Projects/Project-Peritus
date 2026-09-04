//! Fail-closed substitutes for effects excluded from verification-only builds.

use std::path::Path;

use peritus_types::Sha256Digest;

use super::{ProductRunResume, ProductRunner};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

impl ProductRunResume {
    /// Verification-only builds do not serialize effectful continuation state.
    pub fn encode_durable(&self) -> Result<Vec<u8>, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "encode durable product-run continuation",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }

    /// Verification-only builds do not restore effectful continuation state.
    pub fn decode_durable(_bytes: &[u8], _transcript: &str) -> Result<Self, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "restore durable product-run continuation",
            "effectful continuation state is unavailable in a verus_only build",
        ))
    }
}

impl ProductRunner {
    /// Verification-only builds cannot inspect an effectful workspace.
    pub fn candidate_digest(_workspace_root: &Path) -> Result<Sha256Digest, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "compute product-run candidate digest",
            "workspace inspection is unavailable in a verus_only build",
        ))
    }
}
