//! Literal public obligations carried through gates, review, acceptance, and fixer routing.

use core::fmt::Write as _;

use peritus_obligations::{
    DirectEvidence, EvidenceBinding, FailureContext, FailureDisposition, FailureOwner,
    ObligationLimits, ObligationSpec, PublicTaskSource, QualificationReport, RequirementDraft,
    RequirementEvidence, RequirementLedger,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::{ProductRunnerError, ProductRunnerErrorKind, bundle};

/// Exact public requirement ledger used by every qualifying phase.
pub(super) struct RunObligations {
    ledger: RequirementLedger,
}

/// Current acceptance conclusions for one exact candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct QualificationState {
    gates: bool,
    obligations: bool,
    review: bool,
}

impl QualificationState {
    pub(super) const fn new(gates: bool, obligations: bool, review: bool) -> Self {
        Self { gates, obligations, review }
    }

    pub(super) const fn all_satisfied(self) -> bool {
        self.gates && self.obligations && self.review
    }

    pub(super) const fn fixer_disposition(self, recovery_available: bool) -> FailureDisposition {
        let owner = if self.all_satisfied() {
            FailureOwner::HarnessInfrastructure
        } else {
            FailureOwner::CandidateDefect
        };
        owner.disposition(FailureContext::new(false, recovery_available))
    }
}

impl RunObligations {
    pub(super) fn capture(
        transcript: &str,
        conversation_revision: u64,
    ) -> Result<Self, ProductRunnerError> {
        let limits = ObligationLimits::production();
        let source =
            PublicTaskSource::new(transcript.as_bytes().to_vec(), conversation_revision, limits)
                .map_err(invalid)?;
        let spans = clause_spans(transcript, limits.max_clause_bytes());
        let mut drafts = spans
            .into_iter()
            .map(|(start, end)| {
                let id = RequirementId::new(digest(
                    b"peritus-product-obligation-v1",
                    &transcript.as_bytes()[start..end],
                ));
                RequirementDraft::new(id, start, end, ObligationSpec::Hard, Vec::new())
            })
            .collect::<Vec<_>>();
        drafts.sort_by_key(RequirementDraft::id);
        let ledger = RequirementLedger::extract(&source, drafts, limits).map_err(invalid)?;
        Ok(Self { ledger })
    }

    pub(super) fn qualify(
        &self,
        candidate: &CandidateIdentity,
        gates_satisfied: bool,
        review_satisfied: bool,
        evidence_text: &str,
    ) -> Result<QualificationReport, ProductRunnerError> {
        let satisfied = gates_satisfied && review_satisfied;
        let evidence_digest =
            digest(b"peritus-product-obligation-evidence-v1", evidence_text.as_bytes());
        let mut evidence = self
            .ledger
            .entries()
            .iter()
            .map(|entry| {
                let binding = EvidenceBinding::new(
                    entry.id(),
                    self.ledger.digest(),
                    *candidate,
                    evidence_digest,
                    Vec::new(),
                    self.ledger.limits(),
                )
                .map_err(invalid)?;
                Ok(RequirementEvidence::Direct(DirectEvidence::new(binding, satisfied)))
            })
            .collect::<Result<Vec<_>, ProductRunnerError>>()?;
        evidence.sort_by_key(RequirementEvidence::requirement_id);
        peritus_obligations::qualify(&self.ledger, candidate, &[], &evidence).map_err(invalid)
    }

    pub(super) fn render(&self) -> String {
        let mut output = String::from(
            "Literal public obligation ledger (each clause remains acceptance-critical):\n",
        );
        for (index, entry) in self.ledger.entries().iter().enumerate() {
            let clause = String::from_utf8_lossy(entry.clause().exact());
            let _ = write!(output, "  {}. {}", index + 1, clause.trim());
            output.push('\n');
        }
        bundle::limit_text(&output, 256 * 1024)
    }

    pub(super) fn append_report(report: &mut String, qualification: &QualificationReport) {
        let _ = write!(
            report,
            "\nPublic obligations: {}\n  required: {}\n  satisfied: {}\n  missing: {}\n  stale: {}\n  invalid: {}\n",
            if qualification.qualified() { "PASS" } else { "FAIL" },
            qualification.required_count(),
            qualification.satisfied_count(),
            qualification.missing_count(),
            qualification.stale_count(),
            qualification.invalid_count(),
        );
    }
}

fn clause_spans(text: &str, maximum: usize) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let proposed = start.saturating_add(maximum).min(text.len());
        let mut end = text.floor_char_boundary(proposed);
        if end == start {
            end = text.ceil_char_boundary(start.saturating_add(1).min(text.len()));
        }
        spans.push((start, end));
        start = end;
    }
    spans
}

fn digest(domain: &[u8], bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(u64::try_from(domain.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(domain);
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
    Sha256Digest::new(hasher.finalize().into())
}

fn invalid(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "construct public obligation ledger",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use peritus_types::{RunId, WorkspaceId};

    #[test]
    fn exact_public_text_becomes_a_qualified_requirement_only_after_review() {
        let obligations =
            RunObligations::capture("User:\nImplement exact behavior.", 3).expect("obligations");
        let candidate = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            3,
            1,
        )
        .expect("candidate");

        let pending = obligations.qualify(&candidate, true, false, "gates pass").expect("pending");
        let accepted = obligations
            .qualify(&candidate, true, true, "gates pass; review pass")
            .expect("accepted");

        assert!(!pending.qualified());
        assert!(accepted.qualified());
        assert!(obligations.render().contains("Implement exact behavior."));
    }

    #[test]
    fn failed_obligation_routes_only_candidate_defects_to_the_fixer() {
        let obligations =
            RunObligations::capture("User:\nImplement exact behavior.", 1).expect("obligations");
        let candidate = CandidateIdentity::new(
            RunId::new([1; 16]).expect("run"),
            WorkspaceId::new([2; 16]).expect("workspace"),
            Sha256Digest::new([3; 32]),
            1,
            1,
        )
        .expect("candidate");
        let report = obligations.qualify(&candidate, false, false, "failed").expect("report");

        assert_eq!(
            QualificationState::new(false, report.qualified(), false).fixer_disposition(true),
            FailureDisposition::RequestFixer,
        );
    }
}
