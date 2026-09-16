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

impl KnowledgeChange {
    /// Logical classification of an explicitly scoped user clarification.
    pub open spec fn spec_is_user_clarification(self) -> bool {
        self == Self::UserClarification
    }

    /// Whether this change carries explicit clarification targets.
    #[must_use]
    pub const fn is_user_clarification(self) -> (clarification: bool)
        ensures clarification == self.spec_is_user_clarification(),
    {
        matches!(self, Self::UserClarification)
    }
}

/// Complete caller-observed current state used for fail-closed freshness decisions.
#[derive(Debug, Eq, PartialEq)]
pub struct CurrentKnowledgeState {
    candidate: CandidateIdentity,
    sources: Vec<SourceDigest>,
}

impl CurrentKnowledgeState {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// The admitted catalog is nonempty and strictly ordered by exact source identity.
    pub open spec fn spec_well_formed(&self) -> bool {
        self.spec_sources().len() > 0 && crate::model::sources_canonical(self.spec_sources())
    }

    /// Logical view of the exact current candidate.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }

    /// Logical view of the complete current source catalog.
    pub closed spec fn spec_sources(&self) -> Seq<SourceDigest> { self.sources@ }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_candidate() == right.spec_candidate() && left.spec_sources() == right.spec_sources()
    }

    /// Creates a state with a complete canonical source-digest catalog.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, duplicate, or unordered source catalogs.
    pub fn new(
        candidate: CandidateIdentity,
        sources: Vec<SourceDigest>,
        limits: KnowledgeLimits,
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == crate::model::sources_admitted(
                sources@, limits.spec_max_catalog_sources(), false),
            match result {
                Ok(value) => value.spec_candidate() == candidate && value.spec_sources() == sources@
                    && value.spec_well_formed(),
                Err(error) => crate::model::sources_validation_error(
                    sources@, limits.spec_max_catalog_sources(), false, error),
            },
    {
        crate::source::validate_sources(sources.as_slice(), limits.max_catalog_sources(), false)?;
        Ok(Self { candidate, sources })
    }

    /// Exact current candidate observation.
    #[must_use]
    pub const fn candidate(&self) -> (candidate: &CandidateIdentity)
        ensures
            candidate.spec_run_id() == self.spec_candidate().spec_run_id(),
            candidate.spec_workspace_id() == self.spec_candidate().spec_workspace_id(),
            candidate.spec_candidate_digest() == self.spec_candidate().spec_candidate_digest(),
            candidate.spec_conversation_revision()
                == self.spec_candidate().spec_conversation_revision(),
            candidate.spec_checkpoint_sequence()
                == self.spec_candidate().spec_checkpoint_sequence(),
    { &self.candidate }

    /// Complete current source-digest catalog.
    #[must_use]
    pub const fn sources(&self) -> (sources: &[SourceDigest])
        ensures sources@ == self.spec_sources(),
    { self.sources.as_slice() }

    /// Whether a prior source binding exactly matches the current catalog.
    #[must_use]
    pub fn source_is_current(&self, source: SourceDigest) -> (current: bool)
        ensures current == crate::model::source_is_current(self.spec_sources(), source),
    {
        let mut index = 0;
        while index < self.sources.len()
            invariant
                index <= self.sources.len(),
                forall |prior: int| 0 <= prior < index ==>
                    !self.sources@[prior].spec_matches(&source),
            decreases self.sources.len() - index,
        {
            if self.sources[index].matches(&source) {
                assert(crate::model::source_is_current(self.spec_sources(), source)) by {
                    let witness = index as int;
                    assert(self.spec_sources()[witness].spec_matches(&source));
                }
                return true;
            }
            assert(!self.sources@[index as int].spec_matches(&source));
            index += 1;
        }
        false
    }
}

impl Clone for CurrentKnowledgeState {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self { candidate: self.candidate, sources: self.sources.clone() }
    }
}

/// One pure invalidation-planning request.
#[derive(Debug, Eq, PartialEq)]
pub struct InvalidationRequest {
    state: CurrentKnowledgeState,
    change: KnowledgeChange,
    affected_sections: Vec<KnowledgeSectionId>,
}

impl InvalidationRequest {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool {
        Self::inputs_valid(self.spec_change(), self.spec_affected_sections())
    }

    /// Exact change-shape and canonical target admission.
    pub open spec fn inputs_valid(change: KnowledgeChange, affected: Seq<KnowledgeSectionId>) -> bool {
        (change.spec_is_user_clarification() == (affected.len() > 0))
            && crate::model::id_members_valid(affected, None)
    }

    /// Exact change-shape errors take precedence over the first invalid identity pair.
    pub open spec fn construction_error(
        change: KnowledgeChange, affected: Seq<KnowledgeSectionId>, error: KnowledgeError,
    ) -> bool {
        if change.spec_is_user_clarification() && affected.len() == 0 {
            error.spec_plain(KnowledgeErrorKind::EmptyCollection)
        } else if !change.spec_is_user_clarification() && affected.len() > 0 {
            error.spec_plain(KnowledgeErrorKind::InvalidChangeRequest)
        } else { crate::model::id_collection_error(affected, None, error) }
    }

    /// Logical view of the complete current observation.
    pub closed spec fn spec_state(&self) -> CurrentKnowledgeState { self.state }

    /// Logical view of the declared change class.
    pub closed spec fn spec_change(&self) -> KnowledgeChange { self.change }

    /// Logical view of canonical clarification targets.
    pub closed spec fn spec_affected_sections(&self) -> Seq<KnowledgeSectionId> {
        self.affected_sections@
    }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        CurrentKnowledgeState::clone_equivalent(&left.spec_state(), &right.spec_state())
            && left.spec_change() == right.spec_change()
            && left.spec_affected_sections() == right.spec_affected_sections()
    }

    /// Creates a change request with canonical clarification targets.
    ///
    /// # Errors
    ///
    /// User clarification requires at least one target. Other change kinds reject targets.
    pub fn new(
        state: CurrentKnowledgeState,
        change: KnowledgeChange,
        affected_sections: Vec<KnowledgeSectionId>,
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == Self::inputs_valid(change, affected_sections@),
            match result {
            Ok(value) => value.spec_state() == state
                && value.spec_change() == change
                && value.spec_affected_sections() == affected_sections@,
            Err(error) => Self::construction_error(change, affected_sections@, error),
        },
    {
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

    pub(crate) const fn same_revision(state: CurrentKnowledgeState) -> (result: Self)
        ensures
            result.spec_state() == state,
            result.spec_change() == KnowledgeChange::SameRevision,
            result.spec_affected_sections().len() == 0,
    {
        Self { state, change: KnowledgeChange::SameRevision, affected_sections: Vec::new() }
    }

    /// Complete current observations.
    #[must_use]
    pub const fn state(&self) -> (state: &CurrentKnowledgeState)
        ensures
            state.spec_candidate() == self.spec_state().spec_candidate(),
            state.spec_sources() == self.spec_state().spec_sources(),
    { &self.state }

    /// Declared public change class.
    #[must_use]
    pub const fn change(&self) -> (change: KnowledgeChange)
        ensures
            change == self.spec_change(),
            change.spec_is_user_clarification()
                == self.spec_change().spec_is_user_clarification(),
    { self.change }

    /// Canonical affected requirement/design identities.
    #[must_use]
    pub const fn affected_sections(&self) -> (sections: &[KnowledgeSectionId])
        ensures sections@ == self.spec_affected_sections(),
    {
        self.affected_sections.as_slice()
    }

    /// Whether a clarification explicitly affects one section.
    #[must_use]
    pub fn affects(&self, id: KnowledgeSectionId) -> (affected: bool)
        ensures affected == crate::model::clarification_affects(self.spec_affected_sections(), id),
    {
        let mut index = 0;
        while index < self.affected_sections.len()
            invariant
                index <= self.affected_sections.len(),
                forall |prior: int| 0 <= prior < index ==>
                    !self.affected_sections@[prior].spec_matches(&id),
            decreases self.affected_sections.len() - index,
        {
            if self.affected_sections[index].matches(&id) {
                assert(crate::model::clarification_affects(
                    self.spec_affected_sections(), id)) by {
                    let witness = index as int;
                    assert(self.spec_affected_sections()[witness].spec_matches(&id));
                }
                return true;
            }
            assert(!self.affected_sections@[index as int].spec_matches(&id));
            index += 1;
        }
        false
    }
}

impl Clone for InvalidationRequest {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self {
            state: self.state.clone(),
            change: self.change,
            affected_sections: self.affected_sections.clone(),
        }
    }
}

} // verus!
