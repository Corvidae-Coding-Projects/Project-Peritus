//! Fail-closed qualification of exact public obligations against current typed evidence.

mod evaluation;

mod validation;
mod verdict;

use crate::{
    AlternativeGroupId, ConditionId, ConditionObservation, ObligationError, RequirementEvidence,
    RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;

#[doc = " Outcome of matching one requirement against its evidence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceVerdict {
    #[doc = " No evidence was supplied."]
    Missing,
    #[doc = " Evidence belongs to another ledger or candidate revision."]
    Stale,
    #[doc = " Current evidence has the wrong shape or does not satisfy the obligation."]
    Invalid,
    #[doc = " Current evidence exactly satisfies the obligation."]
    Satisfied,
}
#[doc = " Complete deterministic result of qualifying one requirement ledger."]
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
    #[doc = " Whether all active obligations have current satisfying evidence."]
    #[must_use]
    pub const fn qualified(&self) -> bool {
        self.qualified
    }
    #[doc = " Active ordinary obligations plus alternative groups."]
    #[must_use]
    pub const fn required_count(&self) -> usize {
        self.required_count
    }
    #[doc = " Active obligations or groups that are completely satisfied."]
    #[must_use]
    pub const fn satisfied_count(&self) -> usize {
        self.satisfied_count
    }
    #[doc = " Missing evidence observations."]
    #[must_use]
    pub const fn missing_count(&self) -> usize {
        self.missing_count
    }
    #[doc = " Evidence observations bound to an older ledger or candidate."]
    #[must_use]
    pub const fn stale_count(&self) -> usize {
        self.stale_count
    }
    #[doc = " Current observations that do not meet their typed obligation."]
    #[must_use]
    pub const fn invalid_count(&self) -> usize {
        self.invalid_count
    }
    #[doc = " Active non-alternative requirements that remain unsatisfied."]
    #[must_use]
    pub const fn unsatisfied_requirements(&self) -> &[RequirementId] {
        self.unsatisfied_requirements.as_slice()
    }
    #[doc = " Public conditions that were not resolved."]
    #[must_use]
    pub const fn unresolved_conditions(&self) -> &[ConditionId] {
        self.unresolved_conditions.as_slice()
    }
    #[doc = " Alternative groups lacking one complete branch."]
    #[must_use]
    pub const fn incomplete_alternatives(&self) -> &[AlternativeGroupId] {
        self.incomplete_alternatives.as_slice()
    }
}
impl Clone for QualificationReport {
    fn clone(&self) -> Self {
        let unsatisfied_requirements = self.unsatisfied_requirements.clone();
        let unresolved_conditions = self.unresolved_conditions.clone();
        let incomplete_alternatives = self.incomplete_alternatives.clone();
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
#[doc = " Qualifies one exact ledger against current candidate-bound evidence."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Rejects oversized, duplicate, unordered, or unknown evidence and conflicting condition"]
#[doc = " observations. Missing, stale, and unsatisfied known evidence are represented in the report."]
pub fn qualify(
    ledger: &RequirementLedger,
    candidate: &CandidateIdentity,
    conditions: &[ConditionObservation],
    evidence: &[RequirementEvidence],
) -> Result<QualificationReport, ObligationError> {
    validation::validate_inputs(ledger, conditions, evidence)?;
    let report = evaluation::evaluate(ledger, candidate, conditions, evidence);
    Ok(report)
}
