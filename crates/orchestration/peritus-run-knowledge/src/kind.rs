//! Typed run-knowledge categories and their evidence authority.

use vstd::prelude::*;

verus! {

/// Semantic category of one retained run-knowledge section.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KnowledgeSectionKind {
    /// Exact repository inventory observation.
    RepositoryInventory,
    /// Exact map from the task to relevant source files.
    RelevantFileMap,
    /// Reference to literal public requirement clauses and provenance.
    LiteralRequirementLedger,
    /// One bounded design section.
    DesignSection,
    /// Compacted tool output retained only to navigate back to exact observations.
    CompactedToolObservation,
    /// Evidence that one prior finding was resolved for a candidate.
    ResolvedFinding,
    /// Index entry pointing to exact candidate qualification evidence.
    CandidateEvidenceIndex,
    /// Model-authored navigation text with no evidence authority.
    NavigationSummary,
}

impl KnowledgeSectionKind {
    /// Whether this kind depends on the active public conversation revision.
    #[must_use]
    pub const fn depends_on_conversation(self) -> bool {
        matches!(
            self,
            Self::LiteralRequirementLedger | Self::DesignSection | Self::NavigationSummary
        )
    }

    /// Whether this kind depends on exact candidate content.
    #[must_use]
    pub const fn depends_on_candidate(self) -> bool {
        matches!(
            self,
            Self::CompactedToolObservation
                | Self::ResolvedFinding
                | Self::CandidateEvidenceIndex
                | Self::NavigationSummary
        )
    }

    /// Evidence authority fixed by the semantic kind.
    #[must_use]
    pub const fn authority(self) -> KnowledgeAuthority {
        match self {
            Self::CompactedToolObservation | Self::NavigationSummary => {
                KnowledgeAuthority::NavigationOnly
            }
            _ => KnowledgeAuthority::Authoritative,
        }
    }
}

/// Whether a retained section may satisfy an authoritative evidence requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KnowledgeAuthority {
    /// Exact provenance-bearing material eligible for its typed fact domain.
    Authoritative,
    /// Navigation text that may only point a caller back to exact material.
    NavigationOnly,
}

} // verus!
