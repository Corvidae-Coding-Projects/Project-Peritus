//! Provenance binding shared by every retained knowledge section.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, SourceDigest};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Exact workspace, source, conversation, candidate, role, and sequence provenance.
#[derive(Debug, Eq, PartialEq)]
pub struct KnowledgeBinding {
    candidate: CandidateIdentity,
    role: HarnessRole,
    creation_sequence: u64,
    sources: Vec<SourceDigest>,
}

impl KnowledgeBinding {
    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool { self.spec_well_formed() }

    /// Roles for which a knowledge binding is accepted.
    pub open spec fn role_supported(role: HarnessRole) -> bool {
        matches!(role, HarnessRole::Writer | HarnessRole::Reviewer | HarnessRole::Fixer)
    }

    /// Intrinsic admitted shape; caller allocation bounds are not retained in this type.
    pub open spec fn spec_well_formed(&self) -> bool {
        Self::role_supported(self.spec_role()) && self.spec_creation_sequence() > 0
            && self.spec_sources().len() > 0 && crate::model::sources_canonical(self.spec_sources())
    }

    /// Exact constructor admission, without adding a candidate checkpoint restriction.
    pub open spec fn inputs_valid(
        role: HarnessRole, creation_sequence: u64, sources: Seq<SourceDigest>, limits: KnowledgeLimits,
    ) -> bool {
        Self::role_supported(role) && creation_sequence > 0
            && crate::model::sources_admitted(sources, limits.spec_max_sources_per_section(), false)
    }

    /// Exact failure precedence and details from the supplied constructor inputs.
    pub open spec fn construction_error(
        role: HarnessRole, creation_sequence: u64, sources: Seq<SourceDigest>, limits: KnowledgeLimits,
        error: KnowledgeError,
    ) -> bool {
        if !Self::role_supported(role) {
            error.spec_plain(KnowledgeErrorKind::UnsupportedRole)
        } else if creation_sequence == 0 {
            error.spec_plain(KnowledgeErrorKind::ZeroCreationSequence)
        } else {
            crate::model::sources_validation_error(sources, limits.spec_max_sources_per_section(), false, error)
        }
    }

    /// Logical view of the candidate provenance.
    pub closed spec fn spec_candidate(&self) -> CandidateIdentity { self.candidate }

    /// Logical view of the role provenance.
    pub closed spec fn spec_role(&self) -> HarnessRole { self.role }

    /// Logical view of the creation sequence.
    pub closed spec fn spec_creation_sequence(&self) -> u64 { self.creation_sequence }

    /// Logical view of every exact source binding.
    pub closed spec fn spec_sources(&self) -> Seq<SourceDigest> { self.sources@ }

    /// Semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_candidate() == right.spec_candidate()
            && left.spec_role() == right.spec_role()
            && left.spec_creation_sequence() == right.spec_creation_sequence()
            && left.spec_sources() == right.spec_sources()
    }

    /// Creates a fully provenance-bound section binding.
    ///
    /// # Errors
    ///
    /// Rejects unsupported roles, sequence zero, empty/oversized sources, and noncanonical source
    /// identities.
    pub fn new(
        candidate: CandidateIdentity,
        role: HarnessRole,
        creation_sequence: u64,
        sources: Vec<SourceDigest>,
        limits: KnowledgeLimits,
    ) -> (result: Result<Self, KnowledgeError>)
        ensures result.is_ok() == Self::inputs_valid(role, creation_sequence, sources@, limits),
            match result {
            Ok(value) => value.spec_candidate() == candidate
                && value.spec_role() == role
                && value.spec_creation_sequence() == creation_sequence
                && value.spec_sources() == sources@ && value.spec_well_formed(),
            Err(error) => Self::construction_error(role, creation_sequence, sources@, limits, error),
        },
    {
        if !supported_role(role) {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::UnsupportedRole));
        }
        if creation_sequence == 0 {
            return Err(KnowledgeError::plain(KnowledgeErrorKind::ZeroCreationSequence));
        }
        crate::source::validate_sources(
            sources.as_slice(),
            limits.max_sources_per_section(),
            false,
        )?;
        Ok(Self { candidate, role, creation_sequence, sources })
    }

    /// Candidate observation active when this section was produced.
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

    /// Writer, reviewer, or fixer view for which the section was produced.
    #[must_use]
    pub const fn role(&self) -> (role: HarnessRole)
        ensures role == self.spec_role(),
    { self.role }

    /// Monotonic creation sequence within the run.
    #[must_use]
    pub const fn creation_sequence(&self) -> (sequence: u64)
        ensures sequence == self.spec_creation_sequence(),
    { self.creation_sequence }

    /// Exact authoritative source digests used to produce the section.
    #[must_use]
    pub const fn sources(&self) -> (sources: &[SourceDigest])
        ensures sources@ == self.spec_sources(),
    { self.sources.as_slice() }
}

impl Clone for KnowledgeBinding {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        proof { use_type_invariant(self); }
        Self {
            candidate: self.candidate,
            role: self.role,
            creation_sequence: self.creation_sequence,
            sources: self.sources.clone(),
        }
    }
}

pub const fn supported_role(role: HarnessRole) -> (supported: bool)
    ensures supported == KnowledgeBinding::role_supported(role),
{
    matches!(role, HarnessRole::Writer | HarnessRole::Reviewer | HarnessRole::Fixer)
}

/// Returns whether two role discriminants are identical.
pub const fn roles_match(left: HarnessRole, right: HarnessRole) -> (same: bool)
    ensures same == (left == right),
{
    matches!(
        (left, right),
        (HarnessRole::Writer, HarnessRole::Writer)
            | (HarnessRole::Reviewer, HarnessRole::Reviewer)
            | (HarnessRole::Fixer, HarnessRole::Fixer)
            | (HarnessRole::Evaluator, HarnessRole::Evaluator)
            | (HarnessRole::Evolver, HarnessRole::Evolver)
    )
}

} // verus!
