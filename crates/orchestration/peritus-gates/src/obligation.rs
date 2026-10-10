//! Pure D1 bridge from typed public obligations to gate qualification.

use peritus_obligations::{
    ConditionObservation, FailureContext, FailureDisposition, FailureOwner, ObligationError,
    QualificationReport, RequirementEvidence, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;

/// Gate-facing result of evaluating every public obligation for one candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateObligationAssessment {
    report: QualificationReport,
}

impl GateObligationAssessment {
    /// Evaluates one exact public ledger without performing effects.
    ///
    /// # Errors
    ///
    /// Rejects malformed condition or evidence collections. Missing, stale, and unsatisfied known
    /// evidence remain explicit in the returned report.
    pub fn evaluate(
        ledger: &RequirementLedger,
        candidate: &CandidateIdentity,
        conditions: &[ConditionObservation],
        evidence: &[RequirementEvidence],
    ) -> Result<Self, ObligationError> {
        peritus_obligations::qualify(ledger, candidate, conditions, evidence)
            .map(|report| Self { report })
    }

    /// Complete typed qualification report.
    #[must_use]
    pub const fn report(&self) -> &QualificationReport {
        &self.report
    }

    /// Whether D1 may treat public obligations as satisfied for this candidate.
    #[must_use]
    pub const fn permits_gate_pass(&self) -> bool {
        self.report.qualified()
    }
}

/// Routes one typed failure without converting non-code causes into candidate defects.
#[must_use]
pub const fn route_failure(owner: FailureOwner, context: FailureContext) -> FailureDisposition {
    owner.disposition(context)
}
