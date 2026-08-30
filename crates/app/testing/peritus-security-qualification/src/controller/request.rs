//! Strict decoding and authentication of one native probe request.

use serde::Deserialize;

use crate::{ProbeSpec, QualificationLimits, digest_bytes, hex_digest};

use super::args::Options;
use super::error::ControllerError;

const MAX_REQUEST_BYTES: usize = 256 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDocument {
    schema_version: u8,
    subject_id: String,
    probe_id: String,
    target: String,
    candidate: CandidateDocument,
    limits: LimitsDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsDocument {
    duration_millis: u64,
    processes: u32,
    peak_memory_bytes: u64,
    output_bytes: u64,
    artifacts: u32,
}

/// Request facts authenticated against both argv and the closed catalog.
pub(super) struct BoundRequest {
    pub(super) subject_id: String,
    pub(super) request_sha256: String,
    pub(super) source_sha256: String,
    pub(super) spec: ProbeSpec,
    pub(super) limits: QualificationLimits,
}

pub(super) fn decode(bytes: &[u8], options: &Options) -> Result<BoundRequest, ControllerError> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST_BYTES {
        return Err(ControllerError::protocol("request is empty or exceeds its byte bound"));
    }
    let actual_digest = hex_digest(digest_bytes(bytes));
    if actual_digest != options.request_sha256 || !lower_hex(&options.request_sha256, 64) {
        return Err(ControllerError::protocol(
            "request bytes do not match the lowercase argv digest",
        ));
    }
    let document: RequestDocument = serde_json::from_slice(bytes)?;
    if document.schema_version != 1 {
        return Err(ControllerError::protocol("unsupported request schema version"));
    }
    if document.subject_id != options.subject_id
        || document.subject_id.is_empty()
        || document.subject_id.len() > 256
    {
        return Err(ControllerError::protocol("request subject does not match argv"));
    }
    let spec = ProbeSpec::h0_production()
        .iter()
        .copied()
        .find(|candidate| candidate.id().as_str() == document.probe_id)
        .ok_or_else(|| ControllerError::protocol("request probe is outside the H0 catalog"))?;
    if document.target != spec.target().as_str() {
        return Err(ControllerError::protocol("request target does not match the H0 catalog"));
    }
    validate_native_target(spec)?;
    document.candidate.validate()?;
    let limits = QualificationLimits::new(
        document.limits.duration_millis,
        document.limits.processes,
        document.limits.peak_memory_bytes,
        document.limits.output_bytes,
        document.limits.artifacts,
    )
    .map_err(|error| ControllerError::protocol(error.to_string()))?;
    Ok(BoundRequest {
        subject_id: document.subject_id,
        request_sha256: actual_digest,
        source_sha256: document.candidate.source_sha256,
        spec,
        limits,
    })
}

impl CandidateDocument {
    fn validate(&self) -> Result<(), ControllerError> {
        for value in [
            &self.acceptance_spec_id,
            &self.harness_id,
            &self.workspace_id,
            &self.policy_id,
            &self.provider_profile_id,
        ] {
            if !lower_hex(value, 32) || value.bytes().all(|byte| byte == b'0') {
                return Err(ControllerError::protocol("candidate contains an invalid identity"));
            }
        }
        if self.workspace_generation == 0 || self.workspace_revision == 0 {
            return Err(ControllerError::protocol("candidate contains a zero revision component"));
        }
        for value in
            [&self.source_sha256, &self.release_manifest_sha256, &self.qualification_plan_sha256]
        {
            if !lower_hex(value, 64) {
                return Err(ControllerError::protocol("candidate contains an invalid SHA-256"));
            }
        }
        Ok(())
    }
}

fn validate_native_target(spec: ProbeSpec) -> Result<(), ControllerError> {
    let expected = spec.target().as_str();
    if expected == "tier-one-host" || expected == current_platform() {
        Ok(())
    } else {
        Err(ControllerError::protocol(format!(
            "native probe target {expected} does not match {}",
            current_platform()
        )))
    }
}

const fn current_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
