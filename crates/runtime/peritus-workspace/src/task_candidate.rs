//! Source snapshots and explicit candidate handoff operations, independent of agent execution.

use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::path::Path;

mod discard_recovery;
/// Task-owned preimages and exact nested-repository restoration contracts.
pub mod managed;
pub use discard_recovery::DiscardTransactionState;

/// Redaction-safe failure while inspecting or operating a caller-authorized candidate.
#[derive(Debug)]
pub struct CandidateError {
    operation: &'static str,
    detail: String,
}

impl CandidateError {
    /// Returns the exact failing operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Returns its retained diagnostic.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}
impl std::fmt::Display for CandidateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}
impl std::error::Error for CandidateError {}

fn repository(operation: &'static str, detail: impl Into<String>) -> CandidateError {
    CandidateError { operation, detail: detail.into() }
}

/// Combines a committed HEAD and exact repository snapshot in the established candidate format.
#[must_use]
pub fn managed_checkpoint_digest(head: &str, repository: [u8; 32]) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update((head.len() as u64).to_le_bytes());
    hash.update(head.as_bytes());
    hash.update(0_u64.to_le_bytes()); // Root entry's empty relative path.
    hash.update([3]); // Existing Repository content-kind tag.
    hash.update(repository);
    hash.update([0]); // Repository entries have no separate permission field.
    Sha256Digest::new(hash.finalize().into())
}

/// Fingerprints the exact committed HEAD, nested repositories, contents, kinds and permissions.
///
/// # Errors
/// Rejects missing committed HEAD, unreadable sources or unsafe repository metadata.
pub fn identity(root: &Path) -> Result<Sha256Digest, CandidateError> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| repository("resolve candidate base", error.to_string()))?;
    if !output.status.success() {
        return Err(repository(
            "resolve candidate base",
            "managed workspace has no committed HEAD",
        ));
    }
    let head = String::from_utf8(output.stdout)
        .map_err(|_| repository("resolve candidate base", "Git HEAD is not UTF-8"))?;
    if head.trim().is_empty() {
        return Err(repository(
            "resolve candidate base",
            "managed workspace has no committed HEAD",
        ));
    }
    Ok(managed_checkpoint_digest(
        head.trim(),
        managed::ManagedBaseline::repository_fingerprint(root)?,
    ))
}
