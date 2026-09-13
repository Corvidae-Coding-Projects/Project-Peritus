//! Fail-closed qualification of exact public obligations against current typed evidence.

#[cfg(verus_only)]
mod admission;
#[cfg(verus_only)]
mod completion;
mod evaluation;
#[cfg(verus_only)]
mod model;
mod validation;
mod verdict;

#[cfg(verus_only)]
pub use admission::qualification_error;
#[cfg(verus_only)]
pub use completion::qualification_satisfied;

use crate::{
    AlternativeGroupId, ConditionId, ConditionObservation, ObligationError, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use vstd::prelude::*;

verus! {

/// Outcome of matching one requirement against its evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceVerdict {
    /// No evidence was supplied.
    Missing,
    /// Evidence belongs to another ledger or candidate revision.
    Stale,
    /// Current evidence has the wrong shape or does not satisfy the obligation.
    Invalid,
    /// Current evidence exactly satisfies the obligation.
    Satisfied,
}

/// Complete input-defined admission for qualification.
pub open spec fn qualification_inputs_valid(
    ledger: &RequirementLedger,
    conditions: Seq<ConditionObservation>,
    evidence: Seq<RequirementEvidence>,
) -> bool {
    model::qualification_inputs_valid(ledger, conditions, evidence)
}

/// Complete deterministic result of qualifying one requirement ledger.
#[derive(Debug, Eq, PartialEq)]
pub struct QualificationReport {
    qualified: bool,
    required_count: usize,
    satisfied_count: usize,
    missing_count: usize,
    stale_count: usize,
    invalid_count: usize,
    unsatisfied_requirements: Vec<RequirementId>,
    unresolved_conditions: Vec<ConditionId>,
    incomplete_alternatives: Vec<AlternativeGroupId>,
}

impl QualificationReport {
    /// Exact stored qualification verdict.
    pub closed spec fn spec_qualified(&self) -> bool { self.qualified }
    /// Exact active ordinary-obligation plus alternative-group count.
    pub closed spec fn spec_required_count(&self) -> nat { self.required_count as nat }
    /// Exact satisfying ordinary-obligation plus complete-alternative count.
    pub closed spec fn spec_satisfied_count(&self) -> nat { self.satisfied_count as nat }
    /// Exact missing ordinary-evidence count.
    pub closed spec fn spec_missing_count(&self) -> nat { self.missing_count as nat }
    /// Exact stale ordinary-evidence count.
    pub closed spec fn spec_stale_count(&self) -> nat { self.stale_count as nat }
    /// Exact invalid ordinary-evidence count.
    pub closed spec fn spec_invalid_count(&self) -> nat { self.invalid_count as nat }
    /// Exact unsatisfied ordinary requirement identities in ledger order.
    pub closed spec fn spec_unsatisfied_requirements(&self) -> Seq<RequirementId> {
        self.unsatisfied_requirements@
    }
    /// Exact unresolved condition identities in ledger-entry order.
    pub closed spec fn spec_unresolved_conditions(&self) -> Seq<ConditionId> {
        self.unresolved_conditions@
    }
    /// Exact first-seen incomplete alternative groups.
    pub closed spec fn spec_incomplete_alternatives(&self) -> Seq<AlternativeGroupId> {
        self.incomplete_alternatives@
    }

    /// Complete successful report correspondence to supplied ledger, candidate, and evidence.
    pub open spec fn spec_refines(
        &self,
        ledger: &RequirementLedger,
        candidate: &CandidateIdentity,
        conditions: Seq<ConditionObservation>,
        evidence: Seq<RequirementEvidence>,
    ) -> bool {
        model::report_refines(self, ledger, candidate, conditions, evidence)
            && self.spec_qualified() == qualification_satisfied(ledger, candidate, conditions, evidence)
    }

    /// Complete semantic report equality preserved by cloning.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        &&& self.spec_qualified() == other.spec_qualified()
        &&& self.spec_required_count() == other.spec_required_count()
        &&& self.spec_satisfied_count() == other.spec_satisfied_count()
        &&& self.spec_missing_count() == other.spec_missing_count()
        &&& self.spec_stale_count() == other.spec_stale_count()
        &&& self.spec_invalid_count() == other.spec_invalid_count()
        &&& self.spec_unsatisfied_requirements() == other.spec_unsatisfied_requirements()
        &&& self.spec_unresolved_conditions() == other.spec_unresolved_conditions()
        &&& self.spec_incomplete_alternatives() == other.spec_incomplete_alternatives()
    }

    /// Whether all active obligations have current satisfying evidence.
    #[must_use]
    pub const fn qualified(&self) -> (value: bool)
        ensures value == self.spec_qualified(),
    { self.qualified }

    /// Active ordinary obligations plus alternative groups.
    #[must_use]
    pub const fn required_count(&self) -> (value: usize)
        ensures value as nat == self.spec_required_count(),
    { self.required_count }

    /// Active obligations or groups that are completely satisfied.
    #[must_use]
    pub const fn satisfied_count(&self) -> (value: usize)
        ensures value as nat == self.spec_satisfied_count(),
    { self.satisfied_count }

    /// Missing evidence observations.
    #[must_use]
    pub const fn missing_count(&self) -> (value: usize)
        ensures value as nat == self.spec_missing_count(),
    { self.missing_count }

    /// Evidence observations bound to an older ledger or candidate.
    #[must_use]
    pub const fn stale_count(&self) -> (value: usize)
        ensures value as nat == self.spec_stale_count(),
    { self.stale_count }

    /// Current observations that do not meet their typed obligation.
    #[must_use]
    pub const fn invalid_count(&self) -> (value: usize)
        ensures value as nat == self.spec_invalid_count(),
    { self.invalid_count }

    /// Active non-alternative requirements that remain unsatisfied.
    #[must_use]
    pub const fn unsatisfied_requirements(&self) -> (value: &[RequirementId])
        ensures value@ == self.spec_unsatisfied_requirements(),
    { self.unsatisfied_requirements.as_slice() }

    /// Public conditions that were not resolved.
    #[must_use]
    pub const fn unresolved_conditions(&self) -> (value: &[ConditionId])
        ensures value@ == self.spec_unresolved_conditions(),
    { self.unresolved_conditions.as_slice() }

    /// Alternative groups lacking one complete branch.
    #[must_use]
    pub const fn incomplete_alternatives(&self) -> (value: &[AlternativeGroupId])
        ensures value@ == self.spec_incomplete_alternatives(),
    { self.incomplete_alternatives.as_slice() }
}

impl Clone for QualificationReport {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        let unsatisfied_requirements = self.unsatisfied_requirements.clone();
        let unresolved_conditions = self.unresolved_conditions.clone();
        let incomplete_alternatives = self.incomplete_alternatives.clone();
        proof {
            assert(unsatisfied_requirements@ =~= self.unsatisfied_requirements@);
            assert(unresolved_conditions@ =~= self.unresolved_conditions@);
            assert(incomplete_alternatives@ =~= self.incomplete_alternatives@);
        }
        Self {
            qualified: self.qualified,
            required_count: self.required_count,
            satisfied_count: self.satisfied_count,
            missing_count: self.missing_count,
            stale_count: self.stale_count,
            invalid_count: self.invalid_count,
            unsatisfied_requirements,
            unresolved_conditions,
            incomplete_alternatives,
        }
    }
}

/// Qualifies one exact ledger against current candidate-bound evidence.
///
/// # Errors
///
/// Rejects oversized, duplicate, unordered, or unknown evidence and conflicting condition
/// observations. Missing, stale, and unsatisfied known evidence are represented in the report.
pub fn qualify(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> (result: Result<QualificationReport, ObligationError>)
    ensures
        result.is_ok() == qualification_inputs_valid(ledger, conditions@, evidence@),
        match result {
            Ok(report) => report.spec_refines(ledger, candidate, conditions@, evidence@),
            Err(error) => qualification_error(ledger, conditions@, evidence@, &error),
        },
{
    validation::validate_inputs(ledger, conditions, evidence)?;
    let report = evaluation::evaluate(ledger, candidate, conditions, evidence);
    proof { completion::report_verdict_complete(&report, ledger, candidate, conditions@, evidence@); }
    Ok(report)
}

} // verus!
