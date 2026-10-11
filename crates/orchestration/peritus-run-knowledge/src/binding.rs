//! Provenance binding shared by every retained knowledge section.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, SourceDigest};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;

#[doc = " Exact workspace, source, conversation, candidate, role, and sequence provenance."]
#[derive(Debug, Eq, PartialEq)]
pub struct KnowledgeBinding {
    candidate: CandidateIdentity,
    role: HarnessRole,
    creation_sequence: u64,
    sources: Vec<SourceDigest>,
}
impl KnowledgeBinding {
    #[doc = " Creates a fully provenance-bound section binding."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects unsupported roles, sequence zero, empty/oversized sources, and noncanonical source"]
    #[doc = " identities."]
    pub fn new(
        candidate: CandidateIdentity,
        role: HarnessRole,
        creation_sequence: u64,
        sources: Vec<SourceDigest>,
        limits: KnowledgeLimits,
    ) -> Result<Self, KnowledgeError> {
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
    #[doc = " Candidate observation active when this section was produced."]
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity {
        &self.candidate
    }
    #[doc = " Writer, reviewer, or fixer view for which the section was produced."]
    #[must_use]
    pub const fn role(&self) -> HarnessRole {
        self.role
    }
    #[doc = " Monotonic creation sequence within the run."]
    #[must_use]
    pub const fn creation_sequence(&self) -> u64 {
        self.creation_sequence
    }
    #[doc = " Exact authoritative source digests used to produce the section."]
    #[must_use]
    pub const fn sources(&self) -> &[SourceDigest] {
        self.sources.as_slice()
    }
}
impl Clone for KnowledgeBinding {
    fn clone(&self) -> Self {
        Self {
            candidate: self.candidate,
            role: self.role,
            creation_sequence: self.creation_sequence,
            sources: self.sources.clone(),
        }
    }
}
pub const fn supported_role(role: HarnessRole) -> bool {
    matches!(role, HarnessRole::Writer | HarnessRole::Reviewer | HarnessRole::Fixer)
}
#[doc = " Returns whether two role discriminants are identical."]
pub const fn roles_match(left: HarnessRole, right: HarnessRole) -> bool {
    matches!(
        (left, right),
        (HarnessRole::Writer, HarnessRole::Writer)
            | (HarnessRole::Reviewer, HarnessRole::Reviewer)
            | (HarnessRole::Fixer, HarnessRole::Fixer)
            | (HarnessRole::Evaluator, HarnessRole::Evaluator)
            | (HarnessRole::Evolver, HarnessRole::Evolver)
    )
}
