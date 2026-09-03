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

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_obligations::{
        DirectEvidence, EvidenceBinding, ObligationLimits, ObligationSpec, PublicTaskSource,
        RequirementDraft, RequirementEvidence, RequirementLedger,
    };
    use peritus_spec::RequirementId;
    use peritus_types::{RunId, Sha256Digest, WorkspaceId};

    const fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::new([byte; 32])
    }

    fn candidate() -> CandidateIdentity {
        CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            digest(3),
            4,
            1,
        )
        .expect("candidate")
    }

    fn ledger() -> RequirementLedger {
        let limits = ObligationLimits::production();
        let source = PublicTaskSource::new(b"The candidate must compile.".to_vec(), 4, limits)
            .expect("source");
        RequirementLedger::extract(
            &source,
            vec![RequirementDraft::new(
                RequirementId::new(digest(5)),
                0,
                source.content().len(),
                ObligationSpec::Hard,
                Vec::new(),
            )],
            limits,
        )
        .expect("ledger")
    }

    #[test]
    fn bridge_passes_only_a_qualified_obligation_report() {
        let candidate = candidate();
        let ledger = ledger();
        let missing = GateObligationAssessment::evaluate(&ledger, &candidate, &[], &[])
            .expect("missing assessment");
        assert!(!missing.permits_gate_pass());

        let binding = EvidenceBinding::new(
            RequirementId::new(digest(5)),
            ledger.digest(),
            candidate,
            digest(6),
            Vec::new(),
            ledger.limits(),
        )
        .expect("binding");
        let evidence = [RequirementEvidence::Direct(DirectEvidence::new(binding, true))];
        let satisfied = GateObligationAssessment::evaluate(&ledger, &candidate, &[], &evidence)
            .expect("satisfied assessment");
        assert!(satisfied.permits_gate_pass());
    }

    #[test]
    fn bridge_never_routes_provider_failure_to_fixer() {
        assert_eq!(
            route_failure(FailureOwner::ProviderFailure, FailureContext::new(false, true)),
            FailureDisposition::RecoverProvider,
        );
    }
}
