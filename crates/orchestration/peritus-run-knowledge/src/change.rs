//! Current observations and explicit invalidation events.

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSectionId, SourceDigest,
};
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Public event that may change which retained sections remain current.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KnowledgeChange {
    /// No authoritative input changed.
    SameRevision,
    /// One or more named requirement or design sections changed with user clarification.
    UserClarification,
    /// Public conversation content changed without a scoped clarification target.
    ConversationRevision,
    /// At least one authoritative source digest changed.
    SourceChanged,
    /// Exact candidate content or its incorporated conversation changed.
    CandidateRevision,
    /// A provider failed without changing repository truth.
    ProviderFailure,
}

/// Complete caller-observed current state used for fail-closed freshness decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentKnowledgeState {
    candidate: CandidateIdentity,
    sources: Vec<SourceDigest>,
}

impl CurrentKnowledgeState {
    /// Creates a state with a complete canonical source-digest catalog.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, duplicate, or unordered source catalogs.
    pub fn new(
        candidate: CandidateIdentity,
        sources: Vec<SourceDigest>,
        limits: KnowledgeLimits,
    ) -> Result<Self, KnowledgeError> {
        crate::source::validate_sources(sources.as_slice(), limits.max_catalog_sources(), false)?;
        Ok(Self { candidate, sources })
    }

    /// Exact current candidate observation.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Complete current source-digest catalog.
    #[must_use]
    pub const fn sources(&self) -> &[SourceDigest] { self.sources.as_slice() }

    /// Whether a prior source binding exactly matches the current catalog.
    #[must_use]
    pub fn source_is_current(&self, source: SourceDigest) -> bool {
        let mut index = 0;
        while index < self.sources.len()
            invariant index <= self.sources.len(),
            decreases self.sources.len() - index,
        {
            if self.sources[index].source_id() == source.source_id() {
                return self.sources[index].content_digest() == source.content_digest();
            }
            if self.sources[index].source_id() > source.source_id() {
                return false;
            }
            index += 1;
        }
        false
    }
}

/// One pure invalidation-planning request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationRequest {
    state: CurrentKnowledgeState,
    change: KnowledgeChange,
    affected_sections: Vec<KnowledgeSectionId>,
}

impl InvalidationRequest {
    /// Creates a change request with canonical clarification targets.
    ///
    /// # Errors
    ///
    /// User clarification requires at least one target. Other change kinds reject targets.
    pub fn new(
        state: CurrentKnowledgeState,
        change: KnowledgeChange,
        affected_sections: Vec<KnowledgeSectionId>,
    ) -> Result<Self, KnowledgeError> {
        if change == KnowledgeChange::UserClarification {
            if affected_sections.is_empty() {
                return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
            }
        } else if !affected_sections.is_empty() {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::InvalidChangeRequest));
        }
        let mut index = 0;
        while index < affected_sections.len()
            invariant index <= affected_sections.len(),
            decreases affected_sections.len() - index,
        {
            if index > 0 {
                if affected_sections[index - 1] == affected_sections[index] {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::DuplicateValue,
                        affected_sections[index],
                    ));
                }
                if affected_sections[index - 1] > affected_sections[index] {
                    return Err(KnowledgeError::section(
                        KnowledgeErrorKind::NonCanonicalOrder,
                        affected_sections[index],
                    ));
                }
            }
            index += 1;
        }
        Ok(Self { state, change, affected_sections })
    }

    pub(crate) const fn same_revision(state: CurrentKnowledgeState) -> Self {
        Self { state, change: KnowledgeChange::SameRevision, affected_sections: Vec::new() }
    }

    /// Complete current observations.
    #[must_use]
    pub const fn state(&self) -> &CurrentKnowledgeState { &self.state }

    /// Declared public change class.
    #[must_use]
    pub const fn change(&self) -> KnowledgeChange { self.change }

    /// Canonical affected requirement/design identities.
    #[must_use]
    pub const fn affected_sections(&self) -> &[KnowledgeSectionId] {
        self.affected_sections.as_slice()
    }

    /// Whether a clarification explicitly affects one section.
    #[must_use]
    pub fn affects(&self, id: KnowledgeSectionId) -> bool {
        let mut index = 0;
        while index < self.affected_sections.len()
            invariant index <= self.affected_sections.len(),
            decreases self.affected_sections.len() - index,
        {
            if self.affected_sections[index] == id {
                return true;
            }
            if self.affected_sections[index] > id {
                return false;
            }
            index += 1;
        }
        false
    }
}

} // verus!
