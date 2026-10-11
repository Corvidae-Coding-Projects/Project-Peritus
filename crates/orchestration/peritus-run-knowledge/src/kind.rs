//! Typed run-knowledge categories and their evidence authority.

#[doc = " Semantic category of one retained run-knowledge section."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum KnowledgeSectionKind {
    #[doc = " Exact repository inventory observation."]
    RepositoryInventory,
    #[doc = " Exact map from the task to relevant source files."]
    RelevantFileMap,
    #[doc = " Reference to literal public requirement clauses and provenance."]
    LiteralRequirementLedger,
    #[doc = " One bounded design section."]
    DesignSection,
    #[doc = " Compacted tool output retained only to navigate back to exact observations."]
    CompactedToolObservation,
    #[doc = " Evidence that one prior finding was resolved for a candidate."]
    ResolvedFinding,
    #[doc = " Index entry pointing to exact candidate qualification evidence."]
    CandidateEvidenceIndex,
    #[doc = " Model-authored navigation text with no evidence authority."]
    NavigationSummary,
}
impl KnowledgeSectionKind {
    #[doc = " Exact semantic equality for executable constructor checks."]
    #[must_use]
    pub(crate) const fn matches(self, other: Self) -> bool {
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
    #[doc = " Whether user clarification may name this section kind directly."]
    #[must_use]
    pub const fn is_clarification_target(self) -> bool {
        matches!(self, Self::LiteralRequirementLedger | Self::DesignSection)
    }
    #[doc = " Whether this kind depends on the active public conversation revision."]
    #[must_use]
    pub const fn depends_on_conversation(self) -> bool {
        matches!(
            self,
            Self::LiteralRequirementLedger | Self::DesignSection | Self::NavigationSummary
        )
    }
    #[doc = " Whether this kind depends on exact candidate content."]
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
    #[doc = " Evidence authority fixed by the semantic kind."]
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
#[doc = " Whether a retained section may satisfy an authoritative evidence requirement."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KnowledgeAuthority {
    #[doc = " Exact provenance-bearing material eligible for its typed fact domain."]
    Authoritative,
    #[doc = " Navigation text that may only point a caller back to exact material."]
    NavigationOnly,
}
impl KnowledgeAuthority {
    #[doc = " Whether this authority is restricted to navigation-only material."]
    #[must_use]
    pub const fn is_navigation_only(self) -> bool {
        matches!(self, Self::NavigationOnly)
    }
}
