//! Current observations and explicit invalidation events.

use crate::{
    KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, KnowledgeSectionId, SourceDigest,
};
use peritus_run_settlement::CandidateIdentity;

#[doc = " Public event that may change which retained sections remain current."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KnowledgeChange {
    #[doc = " No authoritative input changed."]
    SameRevision,
    #[doc = " One or more named requirement or design sections changed with user clarification."]
    UserClarification,
    #[doc = " Public conversation content changed without a scoped clarification target."]
    ConversationRevision,
    #[doc = " At least one authoritative source digest changed."]
    SourceChanged,
    #[doc = " Exact candidate content or its incorporated conversation changed."]
    CandidateRevision,
    #[doc = " A provider failed without changing repository truth."]
    ProviderFailure,
}
impl KnowledgeChange {
    #[doc = " Whether this change carries explicit clarification targets."]
    #[must_use]
    pub const fn is_user_clarification(self) -> bool {
        matches!(self, Self::UserClarification)
    }
}
#[doc = " Complete caller-observed current state used for fail-closed freshness decisions."]
#[derive(Debug, Eq, PartialEq)]
pub struct CurrentKnowledgeState {
    candidate: CandidateIdentity,
    sources: Vec<SourceDigest>,
}
impl CurrentKnowledgeState {
    #[doc = " Creates a state with a complete canonical source-digest catalog."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects empty, oversized, duplicate, or unordered source catalogs."]
    pub fn new(
        candidate: CandidateIdentity,
        sources: Vec<SourceDigest>,
        limits: KnowledgeLimits,
    ) -> Result<Self, KnowledgeError> {
        crate::source::validate_sources(sources.as_slice(), limits.max_catalog_sources(), false)?;
        Ok(Self { candidate, sources })
    }
    #[doc = " Exact current candidate observation."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Complete current source-digest catalog."]
    #[must_use]
    pub const fn sources(&self) -> &[SourceDigest] {
        self.sources.as_slice()
    }
    #[doc = " Whether a prior source binding exactly matches the current catalog."]
    #[must_use]
    pub fn source_is_current(&self, source: SourceDigest) -> bool {
        let mut index = 0;
        while index < self.sources.len() {
            if self.sources[index].matches(&source) {
                return true;
            }
            index += 1;
        }
        false
    }
}
impl Clone for CurrentKnowledgeState {
    fn clone(&self) -> Self {
        Self { candidate: self.candidate, sources: self.sources.clone() }
    }
}
#[doc = " One pure invalidation-planning request."]
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidationRequest {
    state: CurrentKnowledgeState,
    change: KnowledgeChange,
    affected_sections: Vec<KnowledgeSectionId>,
}
impl InvalidationRequest {
    #[doc = " Creates a change request with canonical clarification targets."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " User clarification requires at least one target. Other change kinds reject targets."]
    pub fn new(
        state: CurrentKnowledgeState,
        change: KnowledgeChange,
        affected_sections: Vec<KnowledgeSectionId>,
    ) -> Result<Self, KnowledgeError> {
        if change.is_user_clarification() {
            if affected_sections.is_empty() {
                return Err(KnowledgeError::plain(KnowledgeErrorKind::EmptyCollection));
            }
        } else if !affected_sections.is_empty() {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::InvalidChangeRequest));
        }
        crate::identity::validate_section_ids(affected_sections.as_slice(), None)?;
        Ok(Self { state, change, affected_sections })
    }
    pub(crate) const fn same_revision(state: CurrentKnowledgeState) -> Self {
        Self { state, change: KnowledgeChange::SameRevision, affected_sections: Vec::new() }
    }
    #[doc = " Complete current observations."]
    #[must_use]
    pub const fn state(&self) -> &CurrentKnowledgeState {
        &self.state
    }
    #[doc = " Declared public change class."]
    #[must_use]
    pub const fn change(&self) -> KnowledgeChange {
        self.change
    }
    #[doc = " Canonical affected requirement/design identities."]
    #[must_use]
    pub const fn affected_sections(&self) -> &[KnowledgeSectionId] {
        self.affected_sections.as_slice()
    }
    #[doc = " Whether a clarification explicitly affects one section."]
    #[must_use]
    pub fn affects(&self, id: KnowledgeSectionId) -> bool {
        let mut index = 0;
        while index < self.affected_sections.len() {
            if self.affected_sections[index].matches(&id) {
                return true;
            }
            index += 1;
        }
        false
    }
}
impl Clone for InvalidationRequest {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            change: self.change,
            affected_sections: self.affected_sections.clone(),
        }
    }
}
