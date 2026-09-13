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
    /// Canonical ordinal of every semantic context class.
    pub open spec fn spec_rank(self) -> u8 {
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

    pub(crate) const fn rank(self) -> (rank: u8)
        ensures rank == self.spec_rank(),
    {
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
#[derive(Debug, Eq, PartialEq)]
pub struct ContextClassSet {
    values: Vec<ContextClass>,
}

impl ContextClassSet {
    /// Exact ordered class sequence stored by this set.
    pub closed spec fn spec_values(&self) -> Seq<ContextClass> { self.values@ }

    /// Canonical admission predicate over the supplied sequence.
    pub open spec fn spec_valid(values: Seq<ContextClass>) -> bool {
        values.len() > 0 && forall |index: int| 0 < index < values.len() ==>
            #[trigger] values[index - 1].spec_rank() < #[trigger] values[index].spec_rank()
    }

    /// Exact first invalid pair, including the rejected current class.
    pub open spec fn spec_error(values: Seq<ContextClass>, error: RoleError) -> bool {
        if values.len() == 0 {
            error.spec_plain(RoleErrorKind::EmptyCollection)
        } else {
            exists |index: int| #![trigger values[index].spec_rank()]
                0 < index < values.len()
                && (forall |prior: int| 0 < prior < index ==>
                    #[trigger] values[prior - 1].spec_rank() < #[trigger] values[prior].spec_rank())
                && if values[index - 1] == values[index] {
                    error.spec_class_error(RoleErrorKind::DuplicateValue, values[index])
                } else {
                    values[index - 1].spec_rank() > values[index].spec_rank()
                        && error.spec_class_error(RoleErrorKind::NonCanonicalOrder, values[index])
                }
        }
    }

    /// Validates a nonempty, strictly increasing class sequence.
    ///
    /// # Errors
    ///
    /// Returns a typed error for an empty, duplicate, or noncanonical sequence.
    pub fn new(values: Vec<ContextClass>) -> (result: Result<Self, RoleError>)
        ensures
            result.is_ok() == Self::spec_valid(values@),
            match result {
                Ok(set) => set.spec_values() == values@,
                Err(error) => Self::spec_error(values@, error),
            },
    {
        if values.is_empty() {
            return Err(RoleError::empty_collection());
        }
        let mut index = 1;
        while index < values.len()
            invariant
                1 <= index <= values.len(),
                forall |prior: int| 0 < prior < index ==>
                    #[trigger] values@[prior - 1].spec_rank() < #[trigger] values@[prior].spec_rank(),
            decreases values.len() - index,
        {
            if values[index - 1].rank() == values[index].rank() {
                let error = RoleError::context_class(RoleErrorKind::DuplicateValue, values[index]);
                assert(Self::spec_error(values@, error)) by {
                    let first = index as int;
                    assert(values@[first - 1] == values@[first]);
                }
                return Err(error);
            }
            if values[index - 1].rank() > values[index].rank() {
                let error = RoleError::context_class(
                    RoleErrorKind::NonCanonicalOrder,
                    values[index],
                );
                assert(Self::spec_error(values@, error)) by {
                    let first = index as int;
                    assert(values@[first - 1].spec_rank() > values@[first].spec_rank());
                }
                return Err(error);
            }
            index += 1;
        }
        Ok(Self { values })
    }

    pub(crate) const fn from_canonical(values: Vec<ContextClass>) -> (set: Self)
        ensures set.spec_values() == values@,
    {
        Self { values }
    }

    /// Returns the classes in canonical order.
    #[must_use]
    pub const fn values(&self) -> (values: &[ContextClass])
        ensures values@ == self.spec_values(),
    {
        self.values.as_slice()
    }

    /// Returns whether the set contains `class`.
    #[must_use]
    pub fn contains(&self, class: ContextClass) -> (contains: bool)
        ensures contains == self.spec_values().contains(class),
    {
        let mut index = 0;
        while index < self.values.len()
            invariant
                index <= self.values.len(),
                forall |prior: int| 0 <= prior < index ==> self.values@[prior] != class,
            decreases self.values.len() - index,
        {
            if self.values[index].rank() == class.rank() {
                return true;
            }
            index += 1;
        }
        false
    }
}

impl Clone for ContextClassSet {
    fn clone(&self) -> (result: Self)
        ensures result.spec_values() == self.spec_values(),
    { Self { values: self.values.clone() } }
}

} // verus!
