//! Native case, receipt, evidence, and cleanup interchange.

use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};

use crate::{
    CaseReport, CleanupObservation, EvidenceEntry, EvidenceSet, EvidenceValue, IntegratedCandidate,
    NativeExecutionReceipt, ProbeObservation, ProbeOutcome, ProbeSpec, QualificationError,
    QualificationLimits, ResourceUsage, SafeEvidenceCode, hex_digest,
};

use super::candidate::parse_hex;
use super::interchange;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CaseDocument {
    probe_id: String,
    outcome: String,
    subject_id: Option<String>,
    failures: Vec<String>,
    native_execution: Option<NativeDocument>,
    cleanup: Option<CleanupDocument>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeDocument {
    semantic_outcome: String,
    executor_sha256: String,
    host_fingerprint_sha256: String,
    command_sha256: String,
    exit_code: i32,
    native_sandbox_observed: bool,
    usage: UsageDocument,
    evidence: Vec<EvidenceDocument>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageDocument {
    elapsed_millis: u64,
    process_count: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifact_count: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EvidenceDocument {
    Fact { label: String, value: bool },
    Count { label: String, value: u64 },
    Digest { label: String, sha256: String, bytes: u64 },
    Code { label: String, value: String },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupDocument {
    subject_id: String,
    remaining_processes: u32,
    remaining_paths: u32,
    remaining_mounts: u32,
    remaining_endpoints: u32,
    evidence_sha256: String,
}

impl CaseDocument {
    pub(super) fn from_case(case: &CaseReport) -> Self {
        Self {
            probe_id: case.spec().id().as_str().to_owned(),
            outcome: outcome(case.outcome()),
            subject_id: case.subject_id().map(str::to_owned),
            failures: case.failures().iter().map(failure).map(str::to_owned).collect(),
            native_execution: case.observation().map(NativeDocument::from_observation),
            cleanup: case.cleanup().map(CleanupDocument::from_cleanup),
        }
    }

    pub(super) fn into_ready(
        self,
        candidate: IntegratedCandidate,
        spec: ProbeSpec,
        limits: QualificationLimits,
    ) -> Result<CaseReport, QualificationError> {
        if self.probe_id != spec.id().as_str()
            || self.outcome != "passed"
            || !self.failures.is_empty()
        {
            return Err(interchange("H0 ready shard contains a non-passing or misordered case"));
        }
        let subject_id = self
            .subject_id
            .ok_or_else(|| interchange("H0 ready shard case omitted its fresh subject"))?;
        let observation = self
            .native_execution
            .ok_or_else(|| interchange("H0 ready shard case omitted native execution"))?
            .into_observation(candidate, spec, limits)?;
        let cleanup = self
            .cleanup
            .ok_or_else(|| interchange("H0 ready shard case omitted cleanup"))?
            .into_cleanup(&subject_id)?;
        Ok(CaseReport::new(spec, Some(subject_id), Some(observation), Some(cleanup), Vec::new()))
    }
}

impl NativeDocument {
    fn from_observation(observation: &ProbeObservation) -> Self {
        let receipt = observation.receipt();
        let usage = receipt.usage();
        Self {
            semantic_outcome: match observation.outcome() {
                ProbeOutcome::Passed => "passed",
                ProbeOutcome::Failed => "failed",
                ProbeOutcome::Unsupported => "unsupported",
            }
            .to_owned(),
            executor_sha256: hex_digest(receipt.executor_digest()),
            host_fingerprint_sha256: hex_digest(receipt.host_fingerprint()),
            command_sha256: hex_digest(receipt.command_digest()),
            exit_code: receipt.exit_code(),
            native_sandbox_observed: receipt.native_sandbox_observed(),
            usage: UsageDocument {
                elapsed_millis: usage.elapsed_millis(),
                process_count: usage.process_count(),
                peak_memory_bytes: usage.peak_memory_bytes(),
                output_bytes: usage.output_bytes(),
                artifact_count: usage.artifact_count(),
            },
            evidence: receipt
                .evidence()
                .entries()
                .iter()
                .map(EvidenceDocument::from_entry)
                .collect(),
        }
    }

    fn into_observation(
        self,
        candidate: IntegratedCandidate,
        spec: ProbeSpec,
        limits: QualificationLimits,
    ) -> Result<ProbeObservation, QualificationError> {
        if self.semantic_outcome != "passed" || self.exit_code != 0 {
            return Err(interchange("H0 ready shard native execution was not a successful pass"));
        }
        let usage = ResourceUsage::new(
            self.usage.elapsed_millis,
            self.usage.process_count,
            self.usage.peak_memory_bytes,
            self.usage.output_bytes,
            self.usage.artifact_count,
        );
        if !usage.within(limits) {
            return Err(interchange("H0 ready shard native usage exceeds its declared limits"));
        }
        if spec.requires_native_sandbox() && !self.native_sandbox_observed {
            return Err(interchange("H0 ready shard omitted required native sandbox evidence"));
        }
        let mut evidence = EvidenceSet::new();
        for entry in self.evidence {
            evidence.insert(entry.into_entry()?)?;
        }
        let receipt = NativeExecutionReceipt::from_native_observation(
            Sha256Digest::new(parse_hex(&self.executor_sha256)?),
            Sha256Digest::new(parse_hex(&self.host_fingerprint_sha256)?),
            Sha256Digest::new(parse_hex(&self.command_sha256)?),
            self.exit_code,
            self.native_sandbox_observed,
            usage,
            evidence,
        )?;
        Ok(ProbeObservation::from_native_execution(
            candidate,
            spec.id(),
            ProbeOutcome::Passed,
            receipt,
        ))
    }
}

impl EvidenceDocument {
    fn from_entry(entry: &EvidenceEntry) -> Self {
        let label = entry.label().as_str().to_owned();
        match entry.value() {
            EvidenceValue::Fact(value) => Self::Fact { label, value: *value },
            EvidenceValue::Count(value) => Self::Count { label, value: *value },
            EvidenceValue::Digest { sha256, bytes } => {
                Self::Digest { label, sha256: hex_digest(*sha256), bytes: *bytes }
            }
            EvidenceValue::Code(value) => Self::Code { label, value: value.as_str().to_owned() },
        }
    }

    fn into_entry(self) -> Result<EvidenceEntry, QualificationError> {
        let (label, value) = match self {
            Self::Fact { label, value } => (label, EvidenceValue::Fact(value)),
            Self::Count { label, value } => (label, EvidenceValue::Count(value)),
            Self::Digest { label, sha256, bytes } => (
                label,
                EvidenceValue::Digest { sha256: Sha256Digest::new(parse_hex(&sha256)?), bytes },
            ),
            Self::Code { label, value } => {
                (label, EvidenceValue::Code(SafeEvidenceCode::new(value)?))
            }
        };
        Ok(EvidenceEntry::new(SafeEvidenceCode::new(label)?, value))
    }
}

impl CleanupDocument {
    fn from_cleanup(cleanup: &CleanupObservation) -> Self {
        Self {
            subject_id: cleanup.subject_id().to_owned(),
            remaining_processes: cleanup.remaining_processes(),
            remaining_paths: cleanup.remaining_paths(),
            remaining_mounts: cleanup.remaining_mounts(),
            remaining_endpoints: cleanup.remaining_endpoints(),
            evidence_sha256: hex_digest(cleanup.cleanup_digest()),
        }
    }

    fn into_cleanup(
        self,
        expected_subject: &str,
    ) -> Result<CleanupObservation, QualificationError> {
        if self.subject_id != expected_subject
            || self.remaining_processes != 0
            || self.remaining_paths != 0
            || self.remaining_mounts != 0
            || self.remaining_endpoints != 0
        {
            return Err(interchange("H0 ready shard cleanup is mismatched or incomplete"));
        }
        CleanupObservation::new(
            self.subject_id,
            self.remaining_processes,
            self.remaining_paths,
            self.remaining_mounts,
            self.remaining_endpoints,
            Sha256Digest::new(parse_hex(&self.evidence_sha256)?),
        )
    }
}

fn outcome(value: crate::CaseOutcome) -> String {
    match value {
        crate::CaseOutcome::NotExecuted => "not-executed",
        crate::CaseOutcome::Failed => "failed",
        crate::CaseOutcome::Passed => "passed",
    }
    .to_owned()
}

const fn failure(value: &crate::CaseFailure) -> &'static str {
    match value {
        crate::CaseFailure::Cancelled => "cancelled",
        crate::CaseFailure::Provision(_) => "provision",
        crate::CaseFailure::AdapterPanicked(_) => "adapter-panicked",
        crate::CaseFailure::NativeExecution(_) => "native-execution",
        crate::CaseFailure::CandidateMismatch => "candidate-mismatch",
        crate::CaseFailure::ProbeMismatch => "probe-mismatch",
        crate::CaseFailure::SubjectReused => "subject-reused",
        crate::CaseFailure::ResourceLimitExceeded { .. } => "resource-limit-exceeded",
        crate::CaseFailure::NativeSandboxNotObserved => "native-sandbox-not-observed",
        crate::CaseFailure::Cleanup(_) => "cleanup-error",
        crate::CaseFailure::CleanupSubjectMismatch => "cleanup-subject-mismatch",
        crate::CaseFailure::CleanupIncomplete => "cleanup-incomplete",
        crate::CaseFailure::Unsupported => "unsupported",
        crate::CaseFailure::AssertionFailed => "assertion-failed",
    }
}
