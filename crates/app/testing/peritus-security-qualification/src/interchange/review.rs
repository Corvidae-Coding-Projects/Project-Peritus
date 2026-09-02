//! Strict external-review JSON admission for final H0 reduction.

use peritus_security_policy::{
    FindingLifecycle, FindingObservation, FindingSeverity, IndependentSecurityReview,
    IntegratedCandidate, ReviewCompletion, ReviewScope, ReviewerIdentity,
};
use peritus_types::{ActorId, FindingId, Sha256Digest};
use serde::Deserialize;

use crate::QualificationError;

use super::candidate::{CandidateDocument, parse_hex};
use super::interchange;

const MAX_REVIEW_BYTES: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewDocument {
    candidate: CandidateDocument,
    reviewer_actor: String,
    reviewer_organization_sha256: String,
    review_context_sha256: String,
    producer_actor: String,
    producer_organization_sha256: String,
    completion: String,
    scopes: Vec<String>,
    independent_from_producer: bool,
    report_sha256: String,
    findings: Vec<FindingDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindingDocument {
    finding_id: String,
    candidate_source_sha256: String,
    severity: String,
    lifecycle: String,
    authority_sha256: Option<String>,
    remediation_sha256: Option<String>,
    retest_sha256: Option<String>,
}

pub(super) fn decode(bytes: &[u8]) -> Result<IndependentSecurityReview, QualificationError> {
    if bytes.is_empty() || bytes.len() > MAX_REVIEW_BYTES {
        return Err(interchange("H0 external review JSON is empty or exceeds its byte bound"));
    }
    let document: ReviewDocument = serde_json::from_slice(bytes)
        .map_err(|error| interchange(format!("decode external review JSON: {error}")))?;
    let candidate = document.candidate.into_candidate()?;
    let reviewer = ReviewerIdentity::new(
        actor(&document.reviewer_actor, "reviewer")?,
        digest(&document.reviewer_organization_sha256)?,
        digest(&document.review_context_sha256)?,
    );
    let producer_actor = actor(&document.producer_actor, "producer")?;
    let producer_organization = digest(&document.producer_organization_sha256)?;
    let completion = match document.completion.as_str() {
        "incomplete" => ReviewCompletion::Incomplete,
        "completed" => ReviewCompletion::Completed,
        _ => return Err(interchange("H0 external review completion is not canonical")),
    };
    let scopes =
        document.scopes.iter().map(|scope| parse_scope(scope)).collect::<Result<Vec<_>, _>>()?;
    let findings = document
        .findings
        .iter()
        .map(|finding| parse_finding(finding, candidate))
        .collect::<Result<Vec<_>, _>>()?;
    let review = IndependentSecurityReview::new(
        candidate,
        reviewer,
        producer_actor,
        producer_organization,
        completion,
        scopes,
        digest(&document.report_sha256)?,
        findings,
    )
    .map_err(|error| {
        interchange(format!(
            "external review evidence rejected: {:?}/{:?}/{}",
            error.collection(),
            error.kind(),
            error.index()
        ))
    })?;
    if review.independent_from_producer() != document.independent_from_producer {
        return Err(interchange(
            "H0 external review independence claim contradicts its actor or organization",
        ));
    }
    Ok(review)
}

fn parse_finding(
    document: &FindingDocument,
    candidate: IntegratedCandidate,
) -> Result<FindingObservation, QualificationError> {
    if digest(&document.candidate_source_sha256)? != candidate.source_digest() {
        return Err(interchange("H0 finding is bound to another candidate source digest"));
    }
    let finding_id = FindingId::new(parse_hex(&document.finding_id)?)
        .map_err(|_| interchange("H0 finding identity is zero"))?;
    let severity = match document.severity.as_str() {
        "critical" => FindingSeverity::Critical,
        "high" => FindingSeverity::High,
        "medium" => FindingSeverity::Medium,
        "low" => FindingSeverity::Low,
        "informational" => FindingSeverity::Informational,
        _ => return Err(interchange("H0 finding severity is not canonical")),
    };
    let lifecycle = parse_lifecycle(document)?;
    Ok(FindingObservation::new(finding_id, candidate, severity, lifecycle))
}

fn parse_lifecycle(document: &FindingDocument) -> Result<FindingLifecycle, QualificationError> {
    match (
        document.lifecycle.as_str(),
        document.authority_sha256.as_deref(),
        document.remediation_sha256.as_deref(),
        document.retest_sha256.as_deref(),
    ) {
        ("open", None, None, None) => Ok(FindingLifecycle::Open),
        ("accepted-risk", Some(authority), None, None) => {
            Ok(FindingLifecycle::AcceptedRisk { authority_digest: digest(authority)? })
        }
        ("resolved", None, Some(remediation), Some(retest)) => Ok(FindingLifecycle::Resolved {
            remediation_digest: digest(remediation)?,
            retest_digest: digest(retest)?,
        }),
        _ => Err(interchange(
            "H0 finding lifecycle does not match its authority, remediation, and retest evidence",
        )),
    }
}

fn actor(value: &str, role: &str) -> Result<ActorId, QualificationError> {
    ActorId::new(parse_hex(value)?).map_err(|_| interchange(format!("H0 {role} actor is zero")))
}

fn digest(value: &str) -> Result<Sha256Digest, QualificationError> {
    Ok(Sha256Digest::new(parse_hex(value)?))
}

fn parse_scope(value: &str) -> Result<ReviewScope, QualificationError> {
    match value {
        "sandbox-escape" => Ok(ReviewScope::SandboxEscape),
        "authority-isolation" => Ok(ReviewScope::AuthorityIsolation),
        "evolution-and-promotion" => Ok(ReviewScope::EvolutionAndPromotion),
        "supply-chain" => Ok(ReviewScope::SupplyChain),
        "unsafe-and-tcb" => Ok(ReviewScope::UnsafeAndTrustedComputingBase),
        _ => Err(interchange("H0 external review scope is not canonical")),
    }
}
