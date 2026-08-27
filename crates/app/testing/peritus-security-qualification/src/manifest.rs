//! Canonical JSON evidence manifest and stable digest.

use peritus_security_policy::{
    FindingLifecycle, FindingSeverity, IndependentSecurityReview, IntegratedCandidate,
    ReviewCompletion, ReviewScope,
};
use peritus_types::Sha256Digest;
use serde::Serialize;

use crate::{
    CaseFailure, CaseOutcome, CaseReport, QualificationError, QualificationErrorCode,
    QualificationRecovery, QualificationRun, digest_bytes, hex_digest,
};

/// Complete canonical H0 evidence-manifest bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceManifest {
    canonical_json: Vec<u8>,
    digest: Sha256Digest,
}

impl EvidenceManifest {
    /// Encodes one complete run and independently supplied review in stable JSON field order.
    ///
    /// # Errors
    ///
    /// Returns a typed serialization failure. The implementation never substitutes partial bytes.
    pub fn new(
        run: &QualificationRun,
        review: Option<&IndependentSecurityReview>,
    ) -> Result<Self, QualificationError> {
        let wire = ManifestWire::from_run(run, review);
        let canonical_json = serde_json::to_vec(&wire).map_err(|error| {
            QualificationError::new(
                QualificationErrorCode::Manifest,
                QualificationRecovery::Quarantine,
                "encode H0 evidence manifest",
                error.to_string(),
            )
        })?;
        let digest = digest_bytes(&canonical_json);
        Ok(Self { canonical_json, digest })
    }

    /// Borrows exact canonical JSON bytes hashed by [`Self::digest`].
    #[must_use]
    pub fn canonical_json(&self) -> &[u8] {
        &self.canonical_json
    }

    /// Returns SHA-256 of the exact canonical JSON bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[derive(Serialize)]
struct ManifestWire {
    schema: &'static str,
    candidate: CandidateWire,
    limits: LimitsWire,
    cases: Vec<CaseWire>,
    external_review: Option<ReviewWire>,
}

impl ManifestWire {
    fn from_run(run: &QualificationRun, review: Option<&IndependentSecurityReview>) -> Self {
        let limits = run.limits();
        Self {
            schema: "peritus.security-evidence-manifest.v1",
            candidate: CandidateWire::new(run.candidate()),
            limits: LimitsWire {
                max_duration_millis: limits.max_duration_millis(),
                max_processes: limits.max_processes(),
                max_peak_memory_bytes: limits.max_peak_memory_bytes(),
                max_output_bytes: limits.max_output_bytes(),
                max_artifacts: limits.max_artifacts(),
            },
            cases: run.cases().iter().map(CaseWire::from_case).collect(),
            external_review: review.map(ReviewWire::from_review),
        }
    }
}

#[derive(Serialize)]
struct CandidateWire {
    acceptance_spec_id: String,
    harness_id: String,
    workspace_id: String,
    workspace_generation: u64,
    workspace_revision: u64,
    policy_id: String,
    provider_profile_id: String,
    source_sha256: String,
    release_manifest_sha256: String,
    qualification_plan_sha256: String,
}

impl CandidateWire {
    fn new(candidate: IntegratedCandidate) -> Self {
        let revision = candidate.revision();
        Self {
            acceptance_spec_id: crate::digest::hex_identifier(
                revision.acceptance_spec_id().as_bytes(),
            ),
            harness_id: crate::digest::hex_identifier(revision.harness_id().as_bytes()),
            workspace_id: crate::digest::hex_identifier(revision.workspace_id().as_bytes()),
            workspace_generation: revision.workspace_generation().get(),
            workspace_revision: revision.workspace_revision().get(),
            policy_id: crate::digest::hex_identifier(revision.policy_id().as_bytes()),
            provider_profile_id: crate::digest::hex_identifier(
                revision.provider_profile_id().as_bytes(),
            ),
            source_sha256: hex_digest(candidate.source_digest()),
            release_manifest_sha256: hex_digest(candidate.release_manifest_digest()),
            qualification_plan_sha256: hex_digest(candidate.qualification_plan_digest()),
        }
    }
}

#[derive(Serialize)]
#[allow(
    clippy::struct_field_names,
    reason = "field names are fixed by the versioned public evidence-manifest schema"
)]
struct LimitsWire {
    max_duration_millis: u64,
    max_processes: u32,
    max_peak_memory_bytes: u64,
    max_output_bytes: u64,
    max_artifacts: u32,
}

#[derive(Serialize)]
struct CaseWire {
    probe_id: &'static str,
    target: &'static str,
    requirement: &'static str,
    acceptance_criterion: u8,
    outcome: &'static str,
    subject_id: Option<String>,
    failures: Vec<&'static str>,
    native_execution: Option<NativeWire>,
    cleanup: Option<CleanupWire>,
}

impl CaseWire {
    fn from_case(case: &CaseReport) -> Self {
        let spec = case.spec();
        Self {
            probe_id: spec.id().as_str(),
            target: spec.target().as_str(),
            requirement: spec.requirement().as_str(),
            acceptance_criterion: spec.criterion().number(),
            outcome: match case.outcome() {
                CaseOutcome::NotExecuted => "not-executed",
                CaseOutcome::Failed => "failed",
                CaseOutcome::Passed => "passed",
            },
            subject_id: case.subject_id().map(str::to_owned),
            failures: case.failures().iter().map(failure_code).collect(),
            native_execution: case.observation().map(|observation| {
                let receipt = observation.receipt();
                let usage = receipt.usage();
                NativeWire {
                    semantic_outcome: match observation.outcome() {
                        crate::ProbeOutcome::Passed => "passed",
                        crate::ProbeOutcome::Failed => "failed",
                        crate::ProbeOutcome::Unsupported => "unsupported",
                    },
                    executor_sha256: hex_digest(receipt.executor_digest()),
                    host_fingerprint_sha256: hex_digest(receipt.host_fingerprint()),
                    command_sha256: hex_digest(receipt.command_digest()),
                    exit_code: receipt.exit_code(),
                    native_sandbox_observed: receipt.native_sandbox_observed(),
                    elapsed_millis: usage.elapsed_millis(),
                    process_count: usage.process_count(),
                    peak_memory_bytes: usage.peak_memory_bytes(),
                    output_bytes: usage.output_bytes(),
                    artifact_count: usage.artifact_count(),
                    evidence_sha256: hex_digest(receipt.evidence().digest()),
                    evidence_entries: receipt.evidence().entries().len(),
                    evidence_encoded_bytes: receipt.evidence().encoded_bytes(),
                }
            }),
            cleanup: case.cleanup().map(|cleanup| CleanupWire {
                subject_id: cleanup.subject_id().to_owned(),
                complete: cleanup.complete(),
                remaining_processes: cleanup.remaining_processes(),
                remaining_paths: cleanup.remaining_paths(),
                remaining_mounts: cleanup.remaining_mounts(),
                remaining_endpoints: cleanup.remaining_endpoints(),
                evidence_sha256: hex_digest(cleanup.cleanup_digest()),
            }),
        }
    }
}

#[derive(Serialize)]
struct NativeWire {
    semantic_outcome: &'static str,
    executor_sha256: String,
    host_fingerprint_sha256: String,
    command_sha256: String,
    exit_code: i32,
    native_sandbox_observed: bool,
    elapsed_millis: u64,
    process_count: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifact_count: u32,
    evidence_sha256: String,
    evidence_entries: usize,
    evidence_encoded_bytes: usize,
}

#[derive(Serialize)]
struct CleanupWire {
    subject_id: String,
    complete: bool,
    remaining_processes: u32,
    remaining_paths: u32,
    remaining_mounts: u32,
    remaining_endpoints: u32,
    evidence_sha256: String,
}

#[derive(Serialize)]
struct ReviewWire {
    candidate: CandidateWire,
    reviewer_actor: String,
    reviewer_organization_sha256: String,
    review_context_sha256: String,
    producer_actor: String,
    producer_organization_sha256: String,
    completion: &'static str,
    scopes: Vec<&'static str>,
    independent_from_producer: bool,
    report_sha256: String,
    findings: Vec<FindingWire>,
}

impl ReviewWire {
    fn from_review(review: &IndependentSecurityReview) -> Self {
        Self {
            candidate: CandidateWire::new(review.candidate()),
            reviewer_actor: crate::digest::hex_identifier(review.reviewer().actor().as_bytes()),
            reviewer_organization_sha256: hex_digest(review.reviewer().organization()),
            review_context_sha256: hex_digest(review.reviewer().context()),
            producer_actor: crate::digest::hex_identifier(review.producer_actor().as_bytes()),
            producer_organization_sha256: hex_digest(review.producer_organization()),
            completion: match review.completion() {
                ReviewCompletion::Incomplete => "incomplete",
                ReviewCompletion::Completed => "completed",
            },
            scopes: review.scopes().iter().copied().map(review_scope_code).collect(),
            independent_from_producer: review.independent_from_producer(),
            report_sha256: hex_digest(review.report_digest()),
            findings: review.findings().iter().map(FindingWire::from_finding).collect(),
        }
    }
}

const fn review_scope_code(scope: ReviewScope) -> &'static str {
    match scope {
        ReviewScope::SandboxEscape => "sandbox-escape",
        ReviewScope::AuthorityIsolation => "authority-isolation",
        ReviewScope::EvolutionAndPromotion => "evolution-and-promotion",
        ReviewScope::SupplyChain => "supply-chain",
        ReviewScope::UnsafeAndTrustedComputingBase => "unsafe-and-tcb",
    }
}

#[derive(Serialize)]
struct FindingWire {
    finding_id: String,
    candidate_source_sha256: String,
    severity: &'static str,
    lifecycle: &'static str,
    authority_sha256: Option<String>,
    remediation_sha256: Option<String>,
    retest_sha256: Option<String>,
}

impl FindingWire {
    fn from_finding(finding: &peritus_security_policy::FindingObservation) -> Self {
        let (lifecycle, authority, remediation, retest) = match finding.lifecycle() {
            FindingLifecycle::Open => ("open", None, None, None),
            FindingLifecycle::AcceptedRisk { authority_digest } => {
                ("accepted-risk", Some(hex_digest(authority_digest)), None, None)
            }
            FindingLifecycle::Resolved { remediation_digest, retest_digest } => (
                "resolved",
                None,
                Some(hex_digest(remediation_digest)),
                Some(hex_digest(retest_digest)),
            ),
        };
        Self {
            finding_id: crate::digest::hex_identifier(finding.finding_id().as_bytes()),
            candidate_source_sha256: hex_digest(finding.candidate().source_digest()),
            severity: match finding.severity() {
                FindingSeverity::Critical => "critical",
                FindingSeverity::High => "high",
                FindingSeverity::Medium => "medium",
                FindingSeverity::Low => "low",
                FindingSeverity::Informational => "informational",
            },
            lifecycle,
            authority_sha256: authority,
            remediation_sha256: remediation,
            retest_sha256: retest,
        }
    }
}

const fn failure_code(failure: &CaseFailure) -> &'static str {
    match failure {
        CaseFailure::Cancelled => "cancelled",
        CaseFailure::Provision(_) => "provision",
        CaseFailure::AdapterPanicked(_) => "adapter-panicked",
        CaseFailure::NativeExecution(_) => "native-execution",
        CaseFailure::CandidateMismatch => "candidate-mismatch",
        CaseFailure::ProbeMismatch => "probe-mismatch",
        CaseFailure::SubjectReused => "subject-reused",
        CaseFailure::ResourceLimitExceeded { .. } => "resource-limit-exceeded",
        CaseFailure::NativeSandboxNotObserved => "native-sandbox-not-observed",
        CaseFailure::Cleanup(_) => "cleanup-error",
        CaseFailure::CleanupSubjectMismatch => "cleanup-subject-mismatch",
        CaseFailure::CleanupIncomplete => "cleanup-incomplete",
        CaseFailure::Unsupported => "unsupported",
        CaseFailure::AssertionFailed => "assertion-failed",
    }
}
