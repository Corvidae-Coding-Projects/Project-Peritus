//! Canonical nonempty risk classifications.

use crate::{CanonicalCollection, PolicyError};

#[doc = " Security-relevant risk category used by escalation and presentation policy."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RiskClass {
    #[doc = " Read-only observation."]
    Read,
    #[doc = " Scoped workspace write."]
    ScopedWrite,
    #[doc = " Process or tool execution."]
    Execution,
    #[doc = " Network communication."]
    Network,
    #[doc = " Dependency or environment change."]
    DependencyEnvironment,
    #[doc = " Repository-history mutation."]
    RepositoryHistoryMutation,
    #[doc = " Secret use."]
    SecretUse,
    #[doc = " External side effect."]
    ExternalSideEffect,
    #[doc = " Policy waiver or amendment."]
    PolicyAuthority,
    #[doc = " Harness promotion or rollback."]
    HarnessPromotion,
}
impl RiskClass {
    pub(crate) const fn rank(self) -> u8 {
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
#[doc = " Nonempty canonical set of risk classes."]
#[derive(Debug, Eq, PartialEq)]
pub struct RiskSet {
    pub(crate) values: Vec<RiskClass>,
}
impl RiskSet {
    pub(crate) const fn from_derived_values(values: Vec<RiskClass>) -> Self {
        Self { values }
    }
    pub(crate) fn contains(&self, risk: RiskClass) -> bool {
        let target = risk.rank();
        let mut index = 0;
        while index < self.values.len() {
            if self.values[index].rank() == target {
                return true;
            }
            index += 1;
        }
        false
    }
    #[doc = " Validates nonempty, strictly ascending risk classes."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a precise canonical-collection failure."]
    pub fn new(values: Vec<RiskClass>) -> Result<Self, PolicyError> {
        if values.is_empty() {
            return Err(PolicyError::empty_canonical_collection(CanonicalCollection::Risks));
        }
        let mut index = 1;
        while index < values.len() {
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
        Ok(risks)
    }
}
