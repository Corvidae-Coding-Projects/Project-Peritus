//! Durable final H0 verdict with the exact canonical evidence-manifest bytes.

use peritus_security_policy::UnmetSecurityCondition;
use serde::Serialize;

use crate::{
    NotReadyReason, QualificationError, QualificationReport, ReadinessVerdict, hex_digest,
};

use super::interchange;

#[derive(Serialize)]
struct FinalReportDocument {
    schema_version: u8,
    status: &'static str,
    candidate_source_sha256: String,
    evidence_manifest_sha256: String,
    probe_count: usize,
    evidence_manifest_json: String,
    not_ready_reasons: Vec<ReasonDocument>,
}

#[derive(Serialize)]
struct ReasonDocument {
    kind: &'static str,
    code: &'static str,
    subject: Option<String>,
    detail: String,
}

pub(super) fn encode(report: &QualificationReport) -> Result<Vec<u8>, QualificationError> {
    let manifest_json = String::from_utf8(report.manifest().canonical_json().to_vec())
        .map_err(|_| interchange("H0 canonical evidence manifest is not UTF-8 JSON"))?;
    let (status, reasons) = match report.verdict() {
        ReadinessVerdict::Ready(_) => ("ready", Vec::new()),
        ReadinessVerdict::NotReady(reasons) => {
            ("not-ready", reasons.iter().copied().map(ReasonDocument::from_reason).collect())
        }
    };
    let document = FinalReportDocument {
        schema_version: 1,
        status,
        candidate_source_sha256: hex_digest(report.run().candidate().source_digest()),
        evidence_manifest_sha256: hex_digest(report.manifest().digest()),
        probe_count: report.run().cases().len(),
        evidence_manifest_json: manifest_json,
        not_ready_reasons: reasons,
    };
    let mut bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| interchange(format!("encode final H0 report JSON: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}

impl ReasonDocument {
    fn from_reason(reason: NotReadyReason) -> Self {
        match reason {
            NotReadyReason::Probe(probe) => Self {
                kind: "probe",
                code: "probe-not-ready",
                subject: Some(probe.as_str().to_owned()),
                detail: format!("native probe {} did not pass", probe.as_str()),
            },
            NotReadyReason::Policy(condition) => Self {
                kind: "policy",
                code: policy_code(condition),
                subject: None,
                detail: format!("{condition:?}"),
            },
        }
    }
}

const fn policy_code(condition: UnmetSecurityCondition) -> &'static str {
    match condition {
        UnmetSecurityCondition::CandidateMismatch { .. } => "candidate-mismatch",
        UnmetSecurityCondition::MissingRequirement(_) => "missing-requirement",
        UnmetSecurityCondition::RequirementDidNotPass { .. } => "requirement-did-not-pass",
        UnmetSecurityCondition::EmptyRequirementEvidence(_) => "empty-requirement-evidence",
        UnmetSecurityCondition::MissingCriterion(_) => "missing-criterion",
        UnmetSecurityCondition::CriterionDidNotPass { .. } => "criterion-did-not-pass",
        UnmetSecurityCondition::EmptyCriterionEvidence(_) => "empty-criterion-evidence",
        UnmetSecurityCondition::MissingInventory(_) => "missing-inventory",
        UnmetSecurityCondition::InventoryIncomplete(_) => "inventory-incomplete",
        UnmetSecurityCondition::EmptyInventoryDigest(_) => "empty-inventory-digest",
        UnmetSecurityCondition::MissingExternalReview => "missing-external-review",
        UnmetSecurityCondition::ExternalReviewIncomplete => "external-review-incomplete",
        UnmetSecurityCondition::ExternalReviewNotIndependent => "external-review-not-independent",
        UnmetSecurityCondition::EmptyExternalReviewDigest => "empty-external-review-digest",
        UnmetSecurityCondition::MissingExternalReviewScope(_) => "missing-external-review-scope",
        UnmetSecurityCondition::UnresolvedReleaseBlocker { .. } => "unresolved-release-blocker",
        UnmetSecurityCondition::MissingEvidenceArtifact(_) => "missing-evidence-artifact",
        UnmetSecurityCondition::EmptyEvidenceDigest(_) => "empty-evidence-digest",
    }
}
