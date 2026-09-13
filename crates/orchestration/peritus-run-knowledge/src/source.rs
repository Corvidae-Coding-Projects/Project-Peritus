//! Exact content digest for one stable source identity.

mod validation;

pub use validation::validate_sources;

use crate::KnowledgeSourceId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Caller-observed digest of one source path or public input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceDigest {
    source_id: KnowledgeSourceId,
    content_digest: Sha256Digest,
}

impl SourceDigest {
    /// Logical view of the stable source identity.
    pub closed spec fn spec_source_id(&self) -> KnowledgeSourceId { self.source_id }

    /// Logical view of the exact content digest.
    pub closed spec fn spec_content_digest(&self) -> Sha256Digest { self.content_digest }

    /// Exact source-identity and content-digest equality.
    pub open spec fn spec_matches(&self, other: &Self) -> bool {
        self.spec_source_id().spec_matches(&other.spec_source_id())
            && self.spec_content_digest().spec_bytes()
                == other.spec_content_digest().spec_bytes()
    }

    /// Binds a source identity to its exact observed content digest.
    #[must_use]
    pub const fn new(
        source_id: KnowledgeSourceId,
        content_digest: Sha256Digest,
    ) -> (result: Self)
        ensures
            result.spec_source_id() == source_id,
            result.spec_content_digest() == content_digest,
    {
        Self { source_id, content_digest }
    }

    /// Stable source identity.
    #[must_use]
    pub const fn source_id(self) -> (source_id: KnowledgeSourceId)
        ensures source_id == self.spec_source_id(),
    { self.source_id }

    /// Exact content digest supplied by the observing boundary.
    #[must_use]
    pub const fn content_digest(self) -> (content_digest: Sha256Digest)
        ensures content_digest == self.spec_content_digest(),
    { self.content_digest }

    /// Returns whether both the source identity and every digest byte agree.
    #[must_use]
    pub fn matches(&self, other: &Self) -> (matches: bool)
        ensures matches == self.spec_matches(other),
    {
        self.source_id.matches(&other.source_id)
            && crate::identity::bytes_equal(
                self.content_digest.as_bytes(),
                other.content_digest.as_bytes(),
            )
    }
}

} // verus!
