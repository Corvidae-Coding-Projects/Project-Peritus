//! Exact digest-bound raw diff byte ranges.

use super::malformed;
use crate::{AppProtocolError, WorkbenchQuery};
use peritus_types::{RunId, Sha256Digest};

/// Maximum exact raw-diff bytes returned by one request.
pub const MAX_WORKBENCH_REVIEW_DIFF_BYTES: usize = 32 * 1024;

/// Request for one exact byte range from a retained raw diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffBytesQuery {
    query: WorkbenchQuery,
    run: RunId,
    revision: u64,
    candidate_digest: Sha256Digest,
    diff_digest: Sha256Digest,
    offset: u32,
    maximum_bytes: u32,
}

/// One exact bounded byte range from an authorized raw diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchReviewDiffBytes {
    query: WorkbenchReviewDiffBytesQuery,
    total_bytes: u32,
    bytes: Vec<u8>,
}

impl WorkbenchReviewDiffBytes {
    /// Constructs a range after validating its request binding and exact bounds.
    ///
    /// # Errors
    /// Rejects oversized, empty, or out-of-range content.
    pub fn new(
        query: WorkbenchReviewDiffBytesQuery,
        total_bytes: u32,
        bytes: Vec<u8>,
    ) -> Result<Self, AppProtocolError> {
        let end = query
            .offset()
            .checked_add(u32::try_from(bytes.len()).map_err(|_| malformed())?)
            .ok_or_else(malformed)?;
        if query.revision() == 0
            || query.maximum_bytes() == 0
            || query.maximum_bytes() as usize > MAX_WORKBENCH_REVIEW_DIFF_BYTES
            || bytes.len() > query.maximum_bytes() as usize
            || end > total_bytes
            || (query.offset() < total_bytes && bytes.is_empty())
        {
            return Err(malformed());
        }
        Ok(Self { query, total_bytes, bytes })
    }
    /// Returns the exact range request.
    #[must_use]
    pub const fn query(&self) -> WorkbenchReviewDiffBytesQuery {
        self.query
    }
    /// Returns the full raw diff length.
    #[must_use]
    pub const fn total_bytes(&self) -> u32 {
        self.total_bytes
    }
    /// Returns the exact requested bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl WorkbenchReviewDiffBytesQuery {
    /// Creates a query for one bounded range of an immutable, digest-bound diff.
    #[must_use]
    pub const fn new(
        query: WorkbenchQuery,
        run: RunId,
        revision: u64,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        offset: u32,
        maximum_bytes: u32,
    ) -> Self {
        Self { query, run, revision, candidate_digest, diff_digest, offset, maximum_bytes }
    }
    /// Returns the workspace query scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the bound run.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
    /// Returns the bound conversation revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the candidate digest.
    #[must_use]
    pub const fn candidate_digest(self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Returns the raw diff digest.
    #[must_use]
    pub const fn diff_digest(self) -> Sha256Digest {
        self.diff_digest
    }
    /// Returns the requested byte offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
    /// Returns the requested maximum byte count.
    #[must_use]
    pub const fn maximum_bytes(self) -> u32 {
        self.maximum_bytes
    }
}
