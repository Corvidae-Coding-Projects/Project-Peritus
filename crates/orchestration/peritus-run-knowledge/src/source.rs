//! Exact content digest for one stable source identity.

mod validation;

pub use validation::validate_sources;

use crate::KnowledgeSourceId;
use peritus_types::Sha256Digest;

#[doc = " Caller-observed digest of one source path or public input."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceDigest {
    source_id: KnowledgeSourceId,
    content_digest: Sha256Digest,
}
impl SourceDigest {
    #[doc = " Binds a source identity to its exact observed content digest."]
    #[must_use]
    pub const fn new(source_id: KnowledgeSourceId, content_digest: Sha256Digest) -> Self {
        Self { source_id, content_digest }
    }
    #[doc = " Stable source identity."]
    #[must_use]
    pub const fn source_id(self) -> KnowledgeSourceId {
        self.source_id
    }
    #[doc = " Exact content digest supplied by the observing boundary."]
    #[must_use]
    pub const fn content_digest(self) -> Sha256Digest {
        self.content_digest
    }
    #[doc = " Returns whether both the source identity and every digest byte agree."]
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self.source_id.matches(&other.source_id)
            && crate::identity::bytes_equal(
                self.content_digest.as_bytes(),
                other.content_digest.as_bytes(),
            )
    }
}
