//! Context classifications used by role visibility policy.

use crate::{RoleError, RoleErrorKind};
use vstd::prelude::*;

verus! {

/// Stable semantic class used to decide what a role may see and contribute.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ContextClass {
    /// Immutable system or application policy.
    ImmutablePolicy,
    /// Frozen acceptance specification and gate definitions.
    AcceptanceSpecification,
    /// The active user request and explicit amendments.
    ActiveUserRequest,
    /// Repository-local instructions.
    RepositoryInstructions,
    /// Relevant repository source.
    RepositorySource,
    /// Exact candidate diff or tree identity.
    CandidateDiff,
    /// Observed workspace state.
    WorkspaceState,
    /// Gate plans, results, and evidence.
    GateEvidence,
    /// Bounded observations returned by tools.
    ToolObservation,
    /// Derived, scoped memory evidence.
    MemoryEvidence,
    /// Prior typed findings.
    PriorFinding,
    /// Evidence-backed finding resolutions.
    FindingResolution,
    /// Agent progress and completion proposals.
    AgentProgress,
    /// Private model reasoning that is never producer-independent evidence.
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

/// Nonempty canonical set of context classes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextClassSet {
    values: Vec<ContextClass>,
}

impl ContextClassSet {
    /// Validates a nonempty, strictly increasing class sequence.
    ///
    /// # Errors
    ///
    /// Returns a typed error for an empty, duplicate, or noncanonical sequence.
    pub fn new(values: Vec<ContextClass>) -> Result<Self, RoleError> {
        if values.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 1;
        while index < values.len()
            invariant 1 <= index <= values.len(),
            decreases values.len() - index,
        {
            if values[index - 1] == values[index] {
                return Err(RoleError::context_class(RoleErrorKind::DuplicateValue, values[index]));
            }
            if values[index - 1].rank() > values[index].rank() {
                return Err(RoleError::context_class(
                    RoleErrorKind::NonCanonicalOrder,
                    values[index],
                ));
            }
            index += 1;
        }
        Ok(Self { values })
    }

    pub(crate) const fn from_canonical(values: Vec<ContextClass>) -> Self {
        Self { values }
    }

    /// Returns the classes in canonical order.
    #[must_use]
    pub const fn values(&self) -> &[ContextClass] {
        self.values.as_slice()
    }

    /// Returns whether the set contains `class`.
    #[must_use]
    pub fn contains(&self, class: ContextClass) -> bool {
        let mut index = 0;
        while index < self.values.len()
            invariant index <= self.values.len(),
            decreases self.values.len() - index,
        {
            if self.values[index] == class {
                return true;
            }
            index += 1;
        }
        false
    }
}

} // verus!
