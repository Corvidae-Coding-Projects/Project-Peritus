//! Provenance binding shared by every retained knowledge section.

use crate::{KnowledgeError, KnowledgeErrorKind, KnowledgeLimits, SourceDigest};
use peritus_role::HarnessRole;
use peritus_run_settlement::CandidateIdentity;
use vstd::prelude::*;

verus! {

/// Exact workspace, source, conversation, candidate, role, and sequence provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeBinding {
    candidate: CandidateIdentity,
    role: HarnessRole,
    creation_sequence: u64,
    sources: Vec<SourceDigest>,
}

impl KnowledgeBinding {
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

    /// Candidate observation active when this section was produced.
    #[must_use]
    pub const fn candidate(&self) -> &CandidateIdentity { &self.candidate }

    /// Writer, reviewer, or fixer view for which the section was produced.
    #[must_use]
    pub const fn role(&self) -> HarnessRole { self.role }

    /// Monotonic creation sequence within the run.
    #[must_use]
    pub const fn creation_sequence(&self) -> u64 { self.creation_sequence }

    /// Exact authoritative source digests used to produce the section.
    #[must_use]
    pub const fn sources(&self) -> &[SourceDigest] { self.sources.as_slice() }
}

pub const fn supported_role(role: HarnessRole) -> bool {
    matches!(role, HarnessRole::Writer | HarnessRole::Reviewer | HarnessRole::Fixer)
}

} // verus!
