//! Context classifications used by role visibility policy.

use crate::{RoleError, RoleErrorKind};

#[doc = " Stable semantic class used to decide what a role may see and contribute."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContextClass {
    #[doc = " Immutable system or application policy."]
    ImmutablePolicy,
    #[doc = " Frozen acceptance specification and gate definitions."]
    AcceptanceSpecification,
    #[doc = " The active user request and explicit amendments."]
    ActiveUserRequest,
    #[doc = " Repository-local instructions."]
    RepositoryInstructions,
    #[doc = " Relevant repository source."]
    RepositorySource,
    #[doc = " Exact candidate diff or tree identity."]
    CandidateDiff,
    #[doc = " Observed workspace state."]
    WorkspaceState,
    #[doc = " Gate plans, results, and evidence."]
    GateEvidence,
    #[doc = " Bounded observations returned by tools."]
    ToolObservation,
    #[doc = " Derived, scoped memory evidence."]
    MemoryEvidence,
    #[doc = " Prior typed findings."]
    PriorFinding,
    #[doc = " Evidence-backed finding resolutions."]
    FindingResolution,
    #[doc = " Agent progress and completion proposals."]
    AgentProgress,
    #[doc = " Private model reasoning that is never producer-independent evidence."]
    HiddenReasoning,
}
impl ContextClass {
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::ImmutablePolicy => 0,
            Self::AcceptanceSpecification => 1,
            Self::ActiveUserRequest => 2,
            Self::RepositoryInstructions => 3,
            Self::RepositorySource => 4,
            Self::CandidateDiff => 5,
            Self::WorkspaceState => 6,
            Self::GateEvidence => 7,
            Self::ToolObservation => 8,
            Self::MemoryEvidence => 9,
            Self::PriorFinding => 10,
            Self::FindingResolution => 11,
            Self::AgentProgress => 12,
            Self::HiddenReasoning => 13,
        }
    }
}
#[doc = " Nonempty canonical set of context classes."]
#[derive(Debug, Eq, PartialEq)]
pub struct ContextClassSet {
    values: Vec<ContextClass>,
}
impl ContextClassSet {
    #[doc = " Validates a nonempty, strictly increasing class sequence."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for an empty, duplicate, or noncanonical sequence."]
    pub fn new(values: Vec<ContextClass>) -> Result<Self, RoleError> {
        if values.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 1;
        while index < values.len() {
            if values[index - 1].rank() == values[index].rank() {
                let error = RoleError::context_class(RoleErrorKind::DuplicateValue, values[index]);
                return Err(error);
            }
            if values[index - 1].rank() > values[index].rank() {
                let error =
                    RoleError::context_class(RoleErrorKind::NonCanonicalOrder, values[index]);
                return Err(error);
            }
            index += 1;
        }
        Ok(Self { values })
    }
    pub(crate) const fn from_canonical(values: Vec<ContextClass>) -> Self {
        Self { values }
    }
    #[doc = " Returns the classes in canonical order."]
    #[must_use]
    pub const fn values(&self) -> &[ContextClass] {
        self.values.as_slice()
    }
    #[doc = " Returns whether the set contains `class`."]
    #[must_use]
    pub fn contains(&self, class: ContextClass) -> bool {
        let mut index = 0;
        while index < self.values.len() {
            if self.values[index].rank() == class.rank() {
                return true;
            }
            index += 1;
        }
        false
    }
}
impl Clone for ContextClassSet {
    fn clone(&self) -> Self {
        Self { values: self.values.clone() }
    }
}
