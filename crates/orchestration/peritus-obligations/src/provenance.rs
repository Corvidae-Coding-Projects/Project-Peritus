//! Exact public task content and clause provenance.

use crate::{ObligationError, ObligationErrorKind, ObligationLimits};
use peritus_types::Sha256Digest;

#[doc = " Immutable public task source from which requirements are extracted."]
#[derive(Debug, Eq, PartialEq)]
pub struct PublicTaskSource {
    content: Vec<u8>,
    digest: Sha256Digest,
    conversation_revision: u64,
}
impl PublicTaskSource {
    pub(crate) fn from_parts(
        content: Vec<u8>,
        digest: Sha256Digest,
        conversation_revision: u64,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        if content.is_empty() || content.len() > limits.max_source_bytes() {
            Err(ObligationError::numbers(
                ObligationErrorKind::InvalidSource,
                limits.max_source_bytes() as u64,
                content.len() as u64,
            ))
        } else {
            Ok(Self { content, digest, conversation_revision })
        }
    }
    #[doc = " Exact public bytes used for extraction."]
    #[must_use]
    pub const fn content(&self) -> &[u8] {
        self.content.as_slice()
    }
    #[doc = " Digest of the complete public source."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[doc = " Conversation revision containing this source."]
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }
}
#[doc = " Exact location of one public clause in its immutable source."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClauseProvenance {
    source_digest: Sha256Digest,
    conversation_revision: u64,
    ordinal: u32,
    byte_start: usize,
    byte_end: usize,
}
impl ClauseProvenance {
    pub(crate) const fn new(
        source_digest: Sha256Digest,
        conversation_revision: u64,
        ordinal: u32,
        byte_start: usize,
        byte_end: usize,
    ) -> Self {
        Self { source_digest, conversation_revision, ordinal, byte_start, byte_end }
    }
    #[doc = " Complete public source digest."]
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    #[doc = " Conversation revision containing the source."]
    #[must_use]
    pub const fn conversation_revision(self) -> u64 {
        self.conversation_revision
    }
    #[doc = " Stable clause ordinal within the ledger."]
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
    #[doc = " Inclusive byte offset in the public source."]
    #[must_use]
    pub const fn byte_start(self) -> usize {
        self.byte_start
    }
    #[doc = " Exclusive byte offset in the public source."]
    #[must_use]
    pub const fn byte_end(self) -> usize {
        self.byte_end
    }
}
#[doc = " Exact clause bytes plus their public-source provenance."]
#[derive(Debug, Eq, PartialEq)]
pub struct PublicClause {
    exact: Vec<u8>,
    provenance: ClauseProvenance,
}
impl PublicClause {
    pub(crate) const fn new(exact: Vec<u8>, provenance: ClauseProvenance) -> Self {
        Self { exact, provenance }
    }
    #[doc = " Exact public bytes; this is authoritative rather than a paraphrase."]
    #[must_use]
    pub const fn exact(&self) -> &[u8] {
        self.exact.as_slice()
    }
    #[doc = " Immutable source provenance."]
    #[must_use]
    pub const fn provenance(&self) -> ClauseProvenance {
        self.provenance
    }
}
impl Clone for PublicTaskSource {
    fn clone(&self) -> Self {
        let content = self.content.clone();
        Self { content, digest: self.digest, conversation_revision: self.conversation_revision }
    }
}
impl Clone for PublicClause {
    fn clone(&self) -> Self {
        let exact = self.exact.clone();
        Self { exact, provenance: self.provenance }
    }
}

impl PublicTaskSource {
    /// Captures one bounded public task source and computes its exact digest.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized source content.
    pub fn new(
        content: Vec<u8>,
        conversation_revision: u64,
        limits: ObligationLimits,
    ) -> Result<Self, ObligationError> {
        let digest = crate::canonical::sha256(content.as_slice());
        Self::from_parts(content, digest, conversation_revision, limits)
    }
}
