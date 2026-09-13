//! Exact content digest for one stable source identity.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeSourceId};
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
    /// Binds a source identity to its exact observed content digest.
    #[must_use]
    pub const fn new(source_id: KnowledgeSourceId, content_digest: Sha256Digest) -> Self {
        Self { source_id, content_digest }
    }

    /// Stable source identity.
    #[must_use]
    pub const fn source_id(self) -> KnowledgeSourceId { self.source_id }

    /// Exact content digest supplied by the observing boundary.
    #[must_use]
    pub const fn content_digest(self) -> Sha256Digest { self.content_digest }
}

pub fn validate_sources(
    sources: &[SourceDigest],
    maximum: usize,
    allow_empty: bool,
) -> Result<(), KnowledgeError> {
    if !allow_empty && sources.is_empty() {
        return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
    }
    if sources.len() > maximum {
        return Err(KnowledgeError::numbers(
            KnowledgeErrorKind::LimitExceeded,
            maximum as u64,
            sources.len() as u64,
        ));
    }
    let mut index = 0;
    while index < sources.len()
        invariant index <= sources.len(),
        decreases sources.len() - index,
    {
        if index > 0 {
            if sources[index - 1].source_id == sources[index].source_id {
                return Err(KnowledgeError::source(
                    KnowledgeErrorKind::DuplicateValue,
                    sources[index].source_id,
                ));
            }
            if sources[index - 1].source_id > sources[index].source_id {
                return Err(KnowledgeError::source(
                    KnowledgeErrorKind::NonCanonicalOrder,
                    sources[index].source_id,
                ));
            }
        }
        index += 1;
    }
    Ok(())
}

} // verus!
