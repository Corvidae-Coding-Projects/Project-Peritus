//! Borrowed verified facts used by reducer commands.

use peritus_budget::BudgetSnapshot;
use peritus_policy::CapabilityUseTransition;
use peritus_quality_policy::AcceptanceEvidence;
use peritus_spec::AcceptanceContract;
#[cfg(verus_only)]
use peritus_types::{FindingId, ReviewCycleId, RevisionTuple};
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
    /// Exact immutable contract supplied by the caller.
    pub closed spec fn spec_contract(&self) -> &'a AcceptanceContract { self.contract }
    /// Exact optional acceptance observations supplied by the caller.
    pub closed spec fn spec_acceptance_evidence(&self) -> Option<&'a AcceptanceEvidence> {
        self.acceptance_evidence
    }
    /// The supplied evidence satisfies the policy's input-defined acceptance conditions.
    pub open spec fn spec_acceptance_authorized(&self, revision: RevisionTuple) -> bool {
        match self.spec_acceptance_evidence() {
            Some(evidence) => peritus_quality_policy::input_defined_acceptance(
                self.spec_contract(), revision, evidence),
            None => false,
        }
    }
    /// A current supplied waiver is policy-valid and belongs to the review cycle that requested it.
    pub open spec fn spec_waiver_grant_authorized(
        &self,
        revision: RevisionTuple,
        finding_id: FindingId,
        review_cycle_id: ReviewCycleId,
    ) -> bool {
        exists |evidence: &'a AcceptanceEvidence,
            waiver_index: int,
            review_index: int,
            finding_index: int|
            self.spec_acceptance_evidence() == Some(evidence)
            && peritus_quality_policy::current_waiver_at(
                evidence.spec_waivers(), finding_id, revision, waiver_index)
            && peritus_quality_policy::supplied_waiver_valid(
                self.spec_contract(),
                revision,
                evidence,
                evidence.spec_waivers()[waiver_index],
            )
            && peritus_quality_policy::current_finding_at(
                evidence.spec_reviews(),
                finding_id,
                revision,
                review_index,
                finding_index,
            )
            && evidence.spec_reviews()[review_index].spec_cycle_id().spec_bytes()@
                == review_cycle_id.spec_bytes()@
    }

    pub(crate) proof fn establish_waiver_grant_authorized(
        &self,
        evidence: &'a AcceptanceEvidence,
        revision: RevisionTuple,
        finding_id: FindingId,
        review_cycle_id: ReviewCycleId,
        waiver_index: int,
        review_index: int,
        finding_index: int,
    )
        requires
            self.spec_acceptance_evidence() == Some(evidence),
            peritus_quality_policy::current_waiver_at(
                evidence.spec_waivers(), finding_id, revision, waiver_index),
            peritus_quality_policy::supplied_waiver_valid(
                self.spec_contract(), revision, evidence, evidence.spec_waivers()[waiver_index]),
            peritus_quality_policy::current_finding_at(
                evidence.spec_reviews(), finding_id, revision, review_index, finding_index),
            evidence.spec_reviews()[review_index].spec_cycle_id().spec_bytes()@
                == review_cycle_id.spec_bytes()@,
        ensures self.spec_waiver_grant_authorized(revision, finding_id, review_cycle_id),
    {
        reveal(ReducerInputs::spec_acceptance_evidence);
        reveal(ReducerInputs::spec_waiver_grant_authorized);
        assert(exists |candidate_evidence: &'a AcceptanceEvidence,
            candidate_waiver: int,
            candidate_review: int,
            candidate_finding: int|
            self.spec_acceptance_evidence() == Some(candidate_evidence)
            && peritus_quality_policy::current_waiver_at(
                candidate_evidence.spec_waivers(), finding_id, revision, candidate_waiver)
            && peritus_quality_policy::supplied_waiver_valid(
                self.spec_contract(),
                revision,
                candidate_evidence,
                candidate_evidence.spec_waivers()[candidate_waiver],
            )
            && peritus_quality_policy::current_finding_at(
                candidate_evidence.spec_reviews(),
                finding_id,
                revision,
                candidate_review,
                candidate_finding,
            )
            && candidate_evidence.spec_reviews()[candidate_review].spec_cycle_id().spec_bytes()@
                == review_cycle_id.spec_bytes()@) by {
            assert(peritus_quality_policy::current_waiver_at(
                evidence.spec_waivers(), finding_id, revision, waiver_index));
            assert(peritus_quality_policy::current_finding_at(
                evidence.spec_reviews(), finding_id, revision, review_index, finding_index));
        }
    }

    /// Creates empty command inputs around the immutable current contract.
    #[must_use]
    pub const fn new(contract: &'a AcceptanceContract) -> (result: Self)
        ensures
            result.spec_contract() == contract,
            result.spec_acceptance_evidence().is_none(),
    {
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
    pub const fn with_run_budget(self, snapshot: BudgetSnapshot) -> (result: Self)
        ensures
            result.spec_contract() == self.spec_contract(),
            result.spec_acceptance_evidence() == self.spec_acceptance_evidence(),
    {
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
    pub const fn with_attempt_budget(self, snapshot: BudgetSnapshot) -> (result: Self)
        ensures
            result.spec_contract() == self.spec_contract(),
            result.spec_acceptance_evidence() == self.spec_acceptance_evidence(),
    {
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
    pub const fn with_parent_budget(self, snapshot: BudgetSnapshot) -> (result: Self)
        ensures
            result.spec_contract() == self.spec_contract(),
            result.spec_acceptance_evidence() == self.spec_acceptance_evidence(),
    {
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
    pub const fn with_capability_use(self, transition: &'a CapabilityUseTransition) -> (result: Self)
        ensures
            result.spec_contract() == self.spec_contract(),
            result.spec_acceptance_evidence() == self.spec_acceptance_evidence(),
    {
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
    pub const fn with_acceptance_evidence(self, evidence: &'a AcceptanceEvidence) -> (result: Self)
        ensures
            result.spec_contract() == self.spec_contract(),
            result.spec_acceptance_evidence() == Some(evidence),
    {
        Self {
            contract: self.contract,
            run_budget: self.run_budget,
            attempt_budget: self.attempt_budget,
            parent_budget: self.parent_budget,
            capability_use: self.capability_use,
            acceptance_evidence: Some(evidence),
        }
    }

    pub(crate) const fn contract(&self) -> (contract: &'a AcceptanceContract)
        ensures contract == self.spec_contract(),
    { self.contract }
    pub(crate) const fn run_budget(&self) -> Option<BudgetSnapshot> { self.run_budget }
    pub(crate) const fn attempt_budget(&self) -> Option<BudgetSnapshot> { self.attempt_budget }
    pub(crate) const fn parent_budget(&self) -> Option<BudgetSnapshot> { self.parent_budget }
    pub(crate) const fn capability_use(&self) -> Option<&'a CapabilityUseTransition> {
        self.capability_use
    }
    pub(crate) const fn acceptance_evidence(&self) -> (evidence: Option<&'a AcceptanceEvidence>)
        ensures evidence == self.spec_acceptance_evidence(),
    {
        self.acceptance_evidence
    }
}

} // verus!
