//! Bounded JSON protocol between the H0 host adapter and one native probe executor.

use serde::{Deserialize, Serialize};

use crate::{
    EvidenceEntry, EvidenceSet, EvidenceValue, IntegratedCandidate, ProbeOutcome, ProbeRequest,
    QualificationError, ResourceUsage, SafeEvidenceCode, digest_bytes, hex_digest,
};

use super::native_error;

pub(super) const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

#[derive(Serialize)]
pub(super) struct NativeProbeRequestDocument<'a> {
    schema_version: u8,
    subject_id: &'a str,
    probe_id: &'static str,
    target: &'static str,
    candidate: CandidateDocument,
    limits: LimitsDocument,
}

impl<'a> NativeProbeRequestDocument<'a> {
    pub(super) fn encode(
        request: ProbeRequest<'_>,
        subject_id: &'a str,
    ) -> Result<Vec<u8>, QualificationError> {
        let document = Self {
            schema_version: 1,
            subject_id,
            probe_id: request.spec().id().as_str(),
            target: request.spec().target().as_str(),
            candidate: CandidateDocument::new(request.candidate()),
            limits: LimitsDocument::new(request),
        };
        serde_json::to_vec_pretty(&document).map_err(|error| {
            native_error("encode native H0 request", format!("serialize request: {error}"))
        })
    }
}

#[derive(Serialize)]
struct CandidateDocument {
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

impl CandidateDocument {
    fn new(candidate: IntegratedCandidate) -> Self {
        let revision = candidate.revision();
        Self {
            acceptance_spec_id: hex_identifier(revision.acceptance_spec_id().as_bytes()),
            harness_id: hex_identifier(revision.harness_id().as_bytes()),
            workspace_id: hex_identifier(revision.workspace_id().as_bytes()),
            workspace_generation: revision.workspace_generation().get(),
            workspace_revision: revision.workspace_revision().get(),
            policy_id: hex_identifier(revision.policy_id().as_bytes()),
            provider_profile_id: hex_identifier(revision.provider_profile_id().as_bytes()),
            source_sha256: hex_digest(candidate.source_digest()),
            release_manifest_sha256: hex_digest(candidate.release_manifest_digest()),
            qualification_plan_sha256: hex_digest(candidate.qualification_plan_digest()),
        }
    }
}

#[derive(Serialize)]
struct LimitsDocument {
    duration_millis: u64,
    processes: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifacts: u32,
}

impl LimitsDocument {
    const fn new(request: ProbeRequest<'_>) -> Self {
        let limits = request.limits();
        Self {
            duration_millis: limits.max_duration_millis(),
            processes: limits.max_processes(),
            peak_memory_bytes: limits.max_peak_memory_bytes(),
            output_bytes: limits.max_output_bytes(),
            artifacts: limits.max_artifacts(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeProbeResponseDocument {
    schema_version: u8,
    subject_id: String,
    request_sha256: String,
    probe_id: String,
    outcome: String,
    native_sandbox_observed: bool,
    usage: UsageDocument,
    evidence: Vec<EvidenceDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageDocument {
    elapsed_millis: u64,
    process_count: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifact_count: u32,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EvidenceDocument {
    Fact { label: String, value: bool },
    Count { label: String, value: u64 },
    Digest { label: String, sha256: String, bytes: u64 },
    Code { label: String, value: String },
}

pub(super) struct ValidatedResponse {
    pub(super) outcome: ProbeOutcome,
    pub(super) native_sandbox_observed: bool,
    pub(super) usage: ResourceUsage,
    pub(super) evidence: EvidenceSet,
}

impl NativeProbeResponseDocument {
    pub(super) fn parse_and_validate(
        bytes: &[u8],
        request_bytes: &[u8],
        request: ProbeRequest<'_>,
        subject_id: &str,
    ) -> Result<ValidatedResponse, QualificationError> {
        let document: Self = serde_json::from_slice(bytes).map_err(|error| {
            native_error("decode native H0 response", format!("invalid response JSON: {error}"))
        })?;
        if document.schema_version != 1 {
            return Err(native_error(
                "decode native H0 response",
                "unsupported native response schema version",
            ));
        }
        if document.subject_id != subject_id {
            return Err(native_error(
                "validate native H0 response",
                "response subject identity does not match the fresh subject",
            ));
        }
        if document.probe_id != request.spec().id().as_str() {
            return Err(native_error(
                "validate native H0 response",
                "response probe identity does not match the requested probe",
            ));
        }
        let request_digest = hex_digest(digest_bytes(request_bytes));
        if document.request_sha256 != request_digest {
            return Err(native_error(
                "validate native H0 response",
                "response is not bound to the exact request document",
            ));
        }
        let outcome = match document.outcome.as_str() {
            "passed" => ProbeOutcome::Passed,
            "failed" => ProbeOutcome::Failed,
            "unsupported" => ProbeOutcome::Unsupported,
            _ => {
                return Err(native_error(
                    "decode native H0 response",
                    "response outcome is not a canonical H0 value",
                ));
            }
        };
        let usage = ResourceUsage::new(
            document.usage.elapsed_millis,
            document.usage.process_count,
            document.usage.peak_memory_bytes,
            document.usage.output_bytes,
            document.usage.artifact_count,
        );
        let evidence = evidence_set(document.evidence)?;
        Ok(ValidatedResponse {
            outcome,
            native_sandbox_observed: document.native_sandbox_observed,
            usage,
            evidence,
        })
    }
}

fn evidence_set(entries: Vec<EvidenceDocument>) -> Result<EvidenceSet, QualificationError> {
    let mut set = EvidenceSet::new();
    for entry in entries {
        let (label, value) = match entry {
            EvidenceDocument::Fact { label, value } => (label, EvidenceValue::Fact(value)),
            EvidenceDocument::Count { label, value } => (label, EvidenceValue::Count(value)),
            EvidenceDocument::Digest { label, sha256, bytes } => {
                (label, EvidenceValue::Digest { sha256: parse_sha256(&sha256)?, bytes })
            }
            EvidenceDocument::Code { label, value } => {
                (label, EvidenceValue::Code(SafeEvidenceCode::new(value)?))
            }
        };
        set.insert(EvidenceEntry::new(SafeEvidenceCode::new(label)?, value))?;
    }
    Ok(set)
}

fn hex_identifier(bytes: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn parse_sha256(value: &str) -> Result<peritus_types::Sha256Digest, QualificationError> {
    if value.len() != 64 {
        return Err(invalid_sha256());
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] =
            decode_nibble(pair[0])?.checked_shl(4).unwrap_or(0) | decode_nibble(pair[1])?;
    }
    Ok(peritus_types::Sha256Digest::new(bytes))
}

fn decode_nibble(value: u8) -> Result<u8, QualificationError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(invalid_sha256()),
    }
}

fn invalid_sha256() -> QualificationError {
    native_error("decode native H0 response", "evidence digest is not lowercase SHA-256")
}
