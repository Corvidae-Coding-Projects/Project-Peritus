//! Exact public task content and clause provenance.

use crate::{ObligationError, ObligationErrorKind, ObligationLimits};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Immutable public task source from which requirements are extracted.
#[derive(Debug, Eq, PartialEq)]
pub struct PublicTaskSource {
    content: Vec<u8>,
    digest: Sha256Digest,
    conversation_revision: u64,
}

impl PublicTaskSource {
    /// Complete public source bytes.
    pub closed spec fn spec_content(&self) -> Seq<u8> { self.content@ }
    /// Complete supplied source digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }
    /// Exact conversation revision.
    pub closed spec fn spec_conversation_revision(&self) -> u64 { self.conversation_revision }

    /// Complete semantic equality of a public source.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_content() == other.spec_content() && self.spec_digest() == other.spec_digest()
            && self.spec_conversation_revision() == other.spec_conversation_revision()
    }

    pub(crate) fn from_parts(
        content: Vec<u8>,
        digest: Sha256Digest,
        conversation_revision: u64,
        limits: ObligationLimits,
    ) -> (result: Result<Self, ObligationError>)
        ensures result.is_ok() == (0 < content@.len() <= limits.spec_max_source_bytes()),
            match result {
                Ok(value) => value.spec_content() == content@
                    && value.spec_digest() == digest
                    && value.spec_conversation_revision() == conversation_revision,
                Err(error) => error.spec_numbers(ObligationErrorKind::InvalidSource,
                    limits.spec_max_source_bytes() as u64, content@.len() as u64),
            },
    {
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

    /// Exact public bytes used for extraction.
    #[must_use]
    pub const fn content(&self) -> (value: &[u8])
        ensures value@ == self.spec_content(),
    { self.content.as_slice() }

    /// Digest of the complete public source.
    #[must_use]
    pub const fn digest(&self) -> (value: Sha256Digest)
        ensures value == self.spec_digest(),
    { self.digest }

    /// Conversation revision containing this source.
    #[must_use]
    pub const fn conversation_revision(&self) -> (value: u64)
        ensures value == self.spec_conversation_revision(),
    { self.conversation_revision }
}

/// Exact location of one public clause in its immutable source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClauseProvenance {
    source_digest: Sha256Digest,
    conversation_revision: u64,
    ordinal: u32,
    byte_start: usize,
    byte_end: usize,
}

impl ClauseProvenance {
    /// Exact stored source digest.
    pub closed spec fn spec_source_digest(&self) -> Sha256Digest { self.source_digest }
    /// Exact stored conversation revision.
    pub closed spec fn spec_conversation_revision(&self) -> u64 { self.conversation_revision }
    /// Exact stored ordinal.
    pub closed spec fn spec_ordinal(&self) -> u32 { self.ordinal }
    /// Exact stored byte start.
    pub closed spec fn spec_byte_start(&self) -> usize { self.byte_start }
    /// Exact stored byte end.
    pub closed spec fn spec_byte_end(&self) -> usize { self.byte_end }

    pub(crate) const fn new(
        source_digest: Sha256Digest,
        conversation_revision: u64,
        ordinal: u32,
        byte_start: usize,
        byte_end: usize,
    ) -> (value: Self)
        ensures value.spec_source_digest() == source_digest,
            value.spec_conversation_revision() == conversation_revision,
            value.spec_ordinal() == ordinal,
            value.spec_byte_start() == byte_start,
            value.spec_byte_end() == byte_end,
    {
        Self { source_digest, conversation_revision, ordinal, byte_start, byte_end }
    }

    /// Complete public source digest.
    #[must_use]
    pub const fn source_digest(self) -> (value: Sha256Digest)
        ensures value == self.spec_source_digest(),
    { self.source_digest }

    /// Conversation revision containing the source.
    #[must_use]
    pub const fn conversation_revision(self) -> (value: u64)
        ensures value == self.spec_conversation_revision(),
    { self.conversation_revision }

    /// Stable clause ordinal within the ledger.
    #[must_use]
    pub const fn ordinal(self) -> (value: u32)
        ensures value == self.spec_ordinal(),
    { self.ordinal }

    /// Inclusive byte offset in the public source.
    #[must_use]
    pub const fn byte_start(self) -> (value: usize)
        ensures value == self.spec_byte_start(),
    { self.byte_start }

    /// Exclusive byte offset in the public source.
    #[must_use]
    pub const fn byte_end(self) -> (value: usize)
        ensures value == self.spec_byte_end(),
    { self.byte_end }
}

/// Exact clause bytes plus their public-source provenance.
#[derive(Debug, Eq, PartialEq)]
pub struct PublicClause {
    exact: Vec<u8>,
    provenance: ClauseProvenance,
}

impl PublicClause {
    /// Complete public clause bytes.
    pub closed spec fn spec_exact(&self) -> Seq<u8> { self.exact@ }
    /// Exact stored source provenance.
    pub closed spec fn spec_provenance(&self) -> ClauseProvenance { self.provenance }

    /// Complete semantic equality of a public clause.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_exact() == other.spec_exact() && self.spec_provenance() == other.spec_provenance()
    }

    pub(crate) const fn new(exact: Vec<u8>, provenance: ClauseProvenance) -> (value: Self)
        ensures value.spec_exact() == exact@, value.spec_provenance() == provenance,
    {
        Self { exact, provenance }
    }

    /// Exact public bytes; this is authoritative rather than a paraphrase.
    #[must_use]
    pub const fn exact(&self) -> (value: &[u8])
        ensures value@ == self.spec_exact(),
    { self.exact.as_slice() }

    /// Immutable source provenance.
    #[must_use]
    pub const fn provenance(&self) -> (value: ClauseProvenance)
        ensures value == self.spec_provenance(),
    { self.provenance }
}

impl Clone for PublicTaskSource {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        let content = self.content.clone();
        proof { assert(content@ =~= self.content@); }
        Self { content, digest: self.digest, conversation_revision: self.conversation_revision }
    }
}

impl Clone for PublicClause {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        let exact = self.exact.clone();
        proof { assert(exact@ =~= self.exact@); }
        Self { exact, provenance: self.provenance }
    }
}

} // verus!

#[cfg(not(verus_only))]
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
