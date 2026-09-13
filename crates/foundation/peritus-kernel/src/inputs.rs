//! Borrowed verified facts used by reducer commands.

use peritus_budget::BudgetSnapshot;
use peritus_policy::CapabilityUseTransition;
use peritus_quality_policy::AcceptanceEvidence;
use peritus_spec::AcceptanceContract;
use vstd::prelude::*;

verus! {

/// Optional exact external facts required by particular command families.
pub struct ReducerInputs<'a> {
    contract: &'a AcceptanceContract,
    run_budget: Option<BudgetSnapshot>,
    attempt_budget: Option<BudgetSnapshot>,
    parent_budget: Option<BudgetSnapshot>,
    capability_use: Option<&'a CapabilityUseTransition>,
    acceptance_evidence: Option<&'a AcceptanceEvidence>,
}

impl<'a> ReducerInputs<'a> {
    /// Creates empty command inputs around the immutable current contract.
    #[must_use]
    pub const fn new(contract: &'a AcceptanceContract) -> Self {
        Self {
            contract,
            run_budget: None,
            attempt_budget: None,
            parent_budget: None,
            capability_use: None,
            acceptance_evidence: None,
        }
    }

    /// Supplies the exact root budget snapshot for run admission.
    #[must_use]
    pub const fn with_run_budget(self, snapshot: BudgetSnapshot) -> Self {
        Self {
            contract: self.contract,
            run_budget: Some(snapshot),
            attempt_budget: self.attempt_budget,
            parent_budget: self.parent_budget,
            capability_use: self.capability_use,
            acceptance_evidence: self.acceptance_evidence,
        }
    }
    /// Supplies the exact child budget snapshot for attempt admission.
    #[must_use]
    pub const fn with_attempt_budget(self, snapshot: BudgetSnapshot) -> Self {
        Self {
            contract: self.contract,
            run_budget: self.run_budget,
            attempt_budget: Some(snapshot),
            parent_budget: self.parent_budget,
            capability_use: self.capability_use,
            acceptance_evidence: self.acceptance_evidence,
        }
    }
    /// Supplies the exact parent snapshot used to check child containment.
    #[must_use]
    pub const fn with_parent_budget(self, snapshot: BudgetSnapshot) -> Self {
        Self {
            contract: self.contract,
            run_budget: self.run_budget,
            attempt_budget: self.attempt_budget,
            parent_budget: Some(snapshot),
            capability_use: self.capability_use,
            acceptance_evidence: self.acceptance_evidence,
        }
    }
    /// Supplies an exact B1 capability-use transition.
    #[must_use]
    pub const fn with_capability_use(self, transition: &'a CapabilityUseTransition) -> Self {
        Self {
            contract: self.contract,
            run_budget: self.run_budget,
            attempt_budget: self.attempt_budget,
            parent_budget: self.parent_budget,
            capability_use: Some(transition),
            acceptance_evidence: self.acceptance_evidence,
        }
    }
    /// Supplies current exact-revision B2 acceptance evidence.
    #[must_use]
    pub const fn with_acceptance_evidence(self, evidence: &'a AcceptanceEvidence) -> Self {
        Self {
            contract: self.contract,
            run_budget: self.run_budget,
            attempt_budget: self.attempt_budget,
            parent_budget: self.parent_budget,
            capability_use: self.capability_use,
            acceptance_evidence: Some(evidence),
        }
    }

    pub(crate) const fn contract(&self) -> &'a AcceptanceContract { self.contract }
    pub(crate) const fn run_budget(&self) -> Option<BudgetSnapshot> { self.run_budget }
    pub(crate) const fn attempt_budget(&self) -> Option<BudgetSnapshot> { self.attempt_budget }
    pub(crate) const fn parent_budget(&self) -> Option<BudgetSnapshot> { self.parent_budget }
    pub(crate) const fn capability_use(&self) -> Option<&'a CapabilityUseTransition> {
        self.capability_use
    }
    pub(crate) const fn acceptance_evidence(&self) -> Option<&'a AcceptanceEvidence> {
        self.acceptance_evidence
    }
}

} // verus!
