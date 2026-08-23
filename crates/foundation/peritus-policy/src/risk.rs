//! Canonical nonempty risk classifications.

use crate::{CanonicalCollection, PolicyError};
use vstd::prelude::*;

verus! {

/// Returns the first duplicate or descending adjacent risk pair, if any.
pub open spec fn first_risk_order_error(
    values: Seq<RiskClass>,
    index: nat,
) -> Option<crate::PolicyErrorKind>
    decreases values.len() - index,
{
    if index >= values.len() {
        None
    } else if values[index as int - 1].spec_rank() == values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::DuplicateCanonicalValue)
    } else if values[index as int - 1].spec_rank() > values[index as int].spec_rank() {
        Some(crate::PolicyErrorKind::NonCanonicalOrder)
    } else {
        first_risk_order_error(values, index + 1)
    }
}

/// Returns the exact first validation failure for a checked risk set.
pub open spec fn risk_set_validation_error(
    values: Seq<RiskClass>,
) -> Option<crate::PolicyErrorKind> {
    if values.len() == 0 {
        Some(crate::PolicyErrorKind::EmptyCanonicalCollection)
    } else {
        first_risk_order_error(values, 1)
    }
}

/// Security-relevant risk category used by escalation and presentation policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RiskClass {
    /// Read-only observation.
    Read,
    /// Scoped workspace write.
    ScopedWrite,
    /// Process or tool execution.
    Execution,
    /// Network communication.
    Network,
    /// Dependency or environment change.
    DependencyEnvironment,
    /// Repository-history mutation.
    RepositoryHistoryMutation,
    /// Secret use.
    SecretUse,
    /// External side effect.
    ExternalSideEffect,
    /// Policy waiver or amendment.
    PolicyAuthority,
    /// Harness promotion or rollback.
    HarnessPromotion,
}

impl RiskClass {
    /// Returns the canonical risk rank used by exact union specifications.
    pub open spec fn spec_rank(self) -> int {
        match self {
            Self::Read => 0,
            Self::ScopedWrite => 1,
            Self::Execution => 2,
            Self::Network => 3,
            Self::DependencyEnvironment => 4,
            Self::RepositoryHistoryMutation => 5,
            Self::SecretUse => 6,
            Self::ExternalSideEffect => 7,
            Self::PolicyAuthority => 8,
            Self::HarnessPromotion => 9,
        }
    }

    pub(crate) const fn rank(self) -> (rank: u8)
        ensures rank as int == self.spec_rank(),
    {
        match self {
            Self::Read => 0,
            Self::ScopedWrite => 1,
            Self::Execution => 2,
            Self::Network => 3,
            Self::DependencyEnvironment => 4,
            Self::RepositoryHistoryMutation => 5,
            Self::SecretUse => 6,
            Self::ExternalSideEffect => 7,
            Self::PolicyAuthority => 8,
            Self::HarnessPromotion => 9,
        }
    }
}

/// Nonempty canonical set of risk classes.
#[derive(Debug, Eq, PartialEq)]
pub struct RiskSet {
    pub(crate) values: Vec<RiskClass>,
}

impl RiskSet {
    /// Returns whether values are strictly canonical by risk rank; empty is allowed while folding.
    pub open spec fn spec_values_are_sorted(values: Seq<RiskClass>) -> bool {
        forall |index: int| 1 <= index < values.len() ==>
            #[trigger] values[index - 1].spec_rank() < values[index].spec_rank()
    }

    /// Returns whether values are nonempty and strictly canonical by risk rank.
    pub open spec fn spec_values_are_valid(values: Seq<RiskClass>) -> bool {
        values.len() > 0 && Self::spec_values_are_sorted(values)
    }

    #[verifier::type_invariant]
    pub(crate) open spec fn invariant(&self) -> bool {
        Self::spec_values_are_valid(self.values@)
    }

    /// Returns the exact canonical risk sequence used by policy specifications.
    pub closed spec fn spec_values(&self) -> Seq<RiskClass> { self.values@ }

    /// Returns exact risk membership by canonical enum rank.
    pub open spec fn spec_contains(&self, risk: RiskClass) -> bool {
        risk_sequence_contains(self.spec_values(), risk)
    }

    pub(crate) const fn from_derived_values(values: Vec<RiskClass>) -> (risks: Self)
        requires Self::spec_values_are_valid(values@),
        ensures risks.spec_values() == values@,
    { Self { values } }

    pub(crate) proof fn expose_values(&self)
        ensures self.spec_values() == self.values@,
    { reveal(RiskSet::spec_values); }

    pub(crate) proof fn same_values_preserve_membership(&self, other: &Self, risk: RiskClass)
        requires self.spec_values() == other.spec_values(),
        ensures self.spec_contains(risk) == other.spec_contains(risk),
    { reveal(RiskSet::spec_values); }

    pub(crate) fn contains(&self, risk: RiskClass) -> (result: bool)
        ensures result == self.spec_contains(risk),
    {
        proof {
            use_type_invariant(self);
            reveal(RiskSet::spec_values);
        }
        let target = risk.rank();
        let mut index = 0;
        while index < self.values.len()
            invariant
                0 <= index <= self.values.len(),
                target as int == risk.spec_rank(),
                forall |prior: int| 0 <= prior < index ==>
                    #[trigger] self.values@[prior].spec_rank() != risk.spec_rank(),
            decreases self.values.len() - index,
        {
            if self.values[index].rank() == target {
                assert(self.spec_contains(risk)) by {
                    assert(self.spec_values()[index as int] == self.values@[index as int]);
                    assert(self.values@[index as int].spec_rank() == risk.spec_rank());
                    assert(exists |found: int| found == index
                        && 0 <= found < self.spec_values().len()
                        && #[trigger] self.spec_values()[found].spec_rank() == risk.spec_rank());
                }
                return true;
            }
            index += 1;
        }
        false
    }

    /// Validates nonempty, strictly ascending risk classes.
    ///
    /// # Errors
    ///
    /// Returns a precise canonical-collection failure.
    pub fn new(values: Vec<RiskClass>) -> (result: Result<Self, PolicyError>)
        ensures
            match result {
                Ok(risks) => {
                    risk_set_validation_error(values@).is_none()
                        && risks.spec_values() == values@
                }
                Err(error) => {
                    risk_set_validation_error(values@) == Some(error.spec_kind())
                        && error.spec_collection() == Some(CanonicalCollection::Risks)
                        && error.spec_dimension().is_none()
                }
            },
    {
        if values.is_empty() {
            return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Risks));
        }
        let mut index = 1;
        while index < values.len()
            invariant
                1 <= index <= values.len(),
                values@.len() > 0,
                first_risk_order_error(values@, 1)
                    == first_risk_order_error(values@, index as nat),
                forall |prior: int| 1 <= prior < index ==>
                    #[trigger] values@[prior - 1].spec_rank() < values@[prior].spec_rank(),
            decreases values.len() - index,
        {
            let previous = values[index - 1].rank();
            let current = values[index].rank();
            if previous == current {
                return Err(PolicyError::duplicate_canonical_value(CanonicalCollection::Risks));
            }
            if previous > current {
                return Err(PolicyError::non_canonical_order(CanonicalCollection::Risks));
            }
            index += 1;
        }
        let risks = Self { values };
        reveal(RiskSet::spec_values);
        Ok(risks)
    }
}

/// Returns exact risk membership for one canonical sequence.
pub open spec fn risk_sequence_contains(values: Seq<RiskClass>, risk: RiskClass) -> bool {
    exists |index: int| 0 <= index < values.len()
        && #[trigger] values[index].spec_rank() == risk.spec_rank()
}

} // verus!
