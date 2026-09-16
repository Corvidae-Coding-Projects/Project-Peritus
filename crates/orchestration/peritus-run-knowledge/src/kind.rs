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
    /// Exact semantic equality for executable constructor checks.
    #[must_use]
    pub(crate) const fn matches(self, other: Self) -> (matches: bool)
        ensures matches == (self == other),
    {
        matches!(
            (self, other),
            (Self::RepositoryInventory, Self::RepositoryInventory)
                | (Self::RelevantFileMap, Self::RelevantFileMap)
                | (Self::LiteralRequirementLedger, Self::LiteralRequirementLedger)
                | (Self::DesignSection, Self::DesignSection)
                | (Self::CompactedToolObservation, Self::CompactedToolObservation)
                | (Self::ResolvedFinding, Self::ResolvedFinding)
                | (Self::CandidateEvidenceIndex, Self::CandidateEvidenceIndex)
                | (Self::NavigationSummary, Self::NavigationSummary)
        )
    }

    /// Logical classification for user-clarification targets.
    pub open spec fn spec_is_clarification_target(self) -> bool {
        self == Self::LiteralRequirementLedger || self == Self::DesignSection
    }

    /// Whether user clarification may name this section kind directly.
    #[must_use]
    pub const fn is_clarification_target(self) -> (target: bool)
        ensures target == self.spec_is_clarification_target(),
    {
        matches!(self, Self::LiteralRequirementLedger | Self::DesignSection)
    }

    /// Logical conversation-dependency classification.
    pub open spec fn spec_depends_on_conversation(self) -> bool {
        matches!(
            self,
            Self::LiteralRequirementLedger | Self::DesignSection | Self::NavigationSummary
        )
    }

    /// Logical candidate-dependency classification.
    pub open spec fn spec_depends_on_candidate(self) -> bool {
        matches!(
            self,
            Self::CompactedToolObservation
                | Self::ResolvedFinding
                | Self::CandidateEvidenceIndex
                | Self::NavigationSummary
        )
    }

    /// Whether this kind depends on the active public conversation revision.
    #[must_use]
    pub const fn depends_on_conversation(self) -> (depends: bool)
        ensures depends == self.spec_depends_on_conversation(),
    {
        matches!(
            self,
            Self::LiteralRequirementLedger | Self::DesignSection | Self::NavigationSummary
        )
    }

    /// Whether this kind depends on exact candidate content.
    #[must_use]
    pub const fn depends_on_candidate(self) -> (depends: bool)
        ensures depends == self.spec_depends_on_candidate(),
    {
        matches!(
            self,
            Self::CompactedToolObservation
                | Self::ResolvedFinding
                | Self::CandidateEvidenceIndex
                | Self::NavigationSummary
        )
    }

    /// Exact authority classification fixed by semantic kind.
    pub open spec fn spec_authority(self) -> KnowledgeAuthority {
        if self == Self::CompactedToolObservation || self == Self::NavigationSummary {
            KnowledgeAuthority::NavigationOnly
        } else {
            KnowledgeAuthority::Authoritative
        }
    }

    /// Evidence authority fixed by the semantic kind.
    #[must_use]
    pub const fn authority(self) -> (authority: KnowledgeAuthority)
        ensures authority == self.spec_authority(),
    {
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

impl KnowledgeAuthority {
    /// Whether this authority is restricted to navigation-only material.
    #[must_use]
    pub const fn is_navigation_only(self) -> (navigation: bool)
        ensures navigation == (self == Self::NavigationOnly),
    {
        matches!(self, Self::NavigationOnly)
    }
}

} // verus!
