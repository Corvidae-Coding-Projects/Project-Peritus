//! Bounded JSON protocol for one complete native H2 scenario lifecycle.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    CleanupObservation, EvidenceEntry, EvidenceKind, EvidenceText, ObservationOutcome,
    PackageManifest, QualificationError, QualificationTarget, ScenarioObservation, ScenarioSpec,
    digest_bytes,
};

use super::NativeControllerLimits;
use super::native_error;

const SCHEMA_VERSION: u8 = 1;
mod artifact;

use artifact::{evidence_set, parse_sha256, validate_artifact};

#[derive(Serialize)]
pub(super) struct NativeRequestDocument<'a> {
    schema_version: u8,
    subject_id: &'a str,
    scenario: ScenarioDocument,
    target: TargetDocument,
    release: ReleaseDocument<'a>,
    controller_sha256: &'a str,
    limits: LimitsDocument,
}

impl<'a> NativeRequestDocument<'a> {
    pub(super) fn encode(
        subject_id: &'a str,
        controller_sha256: &'a str,
        request: crate::ScenarioRequest<'a>,
        limits: NativeControllerLimits,
    ) -> Result<Vec<u8>, QualificationError> {
        let document = Self {
            schema_version: SCHEMA_VERSION,
            subject_id,
            scenario: ScenarioDocument::new(request.scenario()),
            target: TargetDocument::new(request.target()),
            release: ReleaseDocument::new(request.manifest())?,
            controller_sha256,
            limits: LimitsDocument::new(limits),
        };
        serde_json::to_vec_pretty(&document).map_err(|error| {
            native_error("encode native H2 request", format!("serialize request: {error}"))
        })
    }
}

#[derive(Serialize)]
struct ScenarioDocument {
    id: &'static str,
    category: &'static str,
    required: bool,
    description: &'static str,
}

impl ScenarioDocument {
    const fn new(scenario: ScenarioSpec) -> Self {
        Self {
            id: scenario.id().as_str(),
            category: scenario.category().as_str(),
            required: scenario.required(),
            description: scenario.description(),
        }
    }
}

#[derive(Serialize)]
struct TargetDocument {
    platform: &'static str,
    architecture: &'static str,
    version: VersionDocument,
}

impl TargetDocument {
    const fn new(target: QualificationTarget) -> Self {
        Self {
            platform: target.platform().as_str(),
            architecture: target.architecture().as_str(),
            version: VersionDocument::new(target.version()),
        }
    }
}

#[derive(Serialize)]
struct VersionDocument {
    major: u16,
    minor: u16,
    patch: u16,
    build: u32,
}

impl VersionDocument {
    const fn new(version: crate::PlatformVersion) -> Self {
        Self {
            major: version.major(),
            minor: version.minor(),
            patch: version.patch(),
            build: version.build(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseDocument<'a> {
    version: &'a str,
    manifest_sha256: String,
    layout_sha256: String,
    manifest_toml: &'a str,
}

impl<'a> ReleaseDocument<'a> {
    fn new(manifest: &'a PackageManifest) -> Result<Self, QualificationError> {
        let manifest_toml = std::str::from_utf8(manifest.canonical_bytes()).map_err(|error| {
            native_error("encode native H2 request", format!("manifest UTF-8: {error}"))
        })?;
        Ok(Self {
            version: manifest.release().as_str(),
            manifest_sha256: manifest.digest().to_hex(),
            layout_sha256: manifest.layout_digest().to_hex(),
            manifest_toml,
        })
    }
}

#[derive(Serialize)]
struct LimitsDocument {
    duration_millis: u64,
    output_bytes: u64,
    response_bytes: u64,
    artifact_bytes: u64,
    package_artifact_bytes: u64,
    processes: u32,
}

impl LimitsDocument {
    fn new(limits: NativeControllerLimits) -> Self {
        Self {
            duration_millis: u64::try_from(limits.duration().as_millis()).unwrap_or(u64::MAX),
            output_bytes: limits.output_bytes(),
            response_bytes: limits.response_bytes(),
            artifact_bytes: limits.artifact_bytes(),
            package_artifact_bytes: limits.package_artifact_bytes(),
            processes: limits.processes(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeResponseDocument {
    schema_version: u8,
    subject_id: String,
    scenario_id: String,
    request_sha256: String,
    outcome: String,
    artifact_count: u32,
    evidence: Vec<EvidenceDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeCleanupDocument {
    schema_version: u8,
    subject_id: String,
    scenario_id: String,
    request_sha256: String,
    complete: bool,
    remaining_resources: u32,
    evidence: ArtifactDocument,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EvidenceDocument {
    Fact { label: String, value: bool },
    Count { label: String, value: u64 },
    Text { label: String, value: String },
    Digest { label: String, path: String, sha256: String, bytes: u64 },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDocument {
    path: String,
    sha256: String,
    bytes: u64,
}

pub(super) struct ValidatedResponse {
    pub(super) observation: ScenarioObservation,
    pub(super) artifact_paths: BTreeSet<String>,
    pub(super) artifact_bytes: u64,
}

#[derive(Clone, Copy)]
pub(super) struct ResponseValidation<'a> {
    pub(super) request_bytes: &'a [u8],
    pub(super) subject_id: &'a str,
    pub(super) scenario: ScenarioSpec,
    pub(super) artifact_root: &'a Path,
    pub(super) limits: NativeControllerLimits,
    pub(super) elapsed_millis: u64,
    pub(super) output_bytes: u64,
    pub(super) exit_code: i32,
}

#[derive(Clone, Copy)]
pub(super) struct CleanupValidation<'a> {
    pub(super) request_bytes: &'a [u8],
    pub(super) subject_id: &'a str,
    pub(super) scenario: ScenarioSpec,
    pub(super) artifact_root: &'a Path,
    pub(super) prior_paths: &'a BTreeSet<String>,
    pub(super) prior_bytes: u64,
    pub(super) limits: NativeControllerLimits,
}

impl NativeResponseDocument {
    pub(super) fn parse_and_validate(
        bytes: &[u8],
        validation: ResponseValidation<'_>,
    ) -> Result<ValidatedResponse, QualificationError> {
        let document: Self = serde_json::from_slice(bytes).map_err(|error| {
            native_error("decode native H2 response", format!("invalid response JSON: {error}"))
        })?;
        validate_binding(
            document.schema_version,
            &document.subject_id,
            &document.scenario_id,
            &document.request_sha256,
            validation.request_bytes,
            validation.subject_id,
            validation.scenario,
            "scenario response",
        )?;
        let outcome = match document.outcome.as_str() {
            "passed" => ObservationOutcome::Passed,
            "failed" => ObservationOutcome::Failed,
            "unsupported" => ObservationOutcome::Unsupported,
            _ => {
                return Err(native_error(
                    "decode native H2 response",
                    "response outcome is not a canonical H2 value",
                ));
            }
        };
        let (mut evidence, artifact_paths, artifact_bytes) = evidence_set(
            document.evidence,
            document.artifact_count,
            validation.artifact_root,
            validation.limits.artifact_bytes(),
        )?;
        evidence.insert(EvidenceEntry::new(
            "adapter.controller.elapsed-millis",
            EvidenceKind::Count(validation.elapsed_millis),
        )?)?;
        evidence.insert(EvidenceEntry::new(
            "adapter.controller.output-bytes",
            EvidenceKind::Count(validation.output_bytes),
        )?)?;
        evidence.insert(EvidenceEntry::new(
            "adapter.controller.exit-code",
            EvidenceKind::Text(EvidenceText::new(validation.exit_code.to_string())?),
        )?)?;
        let observation = ScenarioObservation::new(
            validation.scenario.id(),
            validation.subject_id.to_owned(),
            outcome,
            evidence,
        )?;
        Ok(ValidatedResponse { observation, artifact_paths, artifact_bytes })
    }
}

impl NativeCleanupDocument {
    pub(super) fn parse_and_validate(
        bytes: &[u8],
        validation: CleanupValidation<'_>,
    ) -> Result<CleanupObservation, QualificationError> {
        let document: Self = serde_json::from_slice(bytes).map_err(|error| {
            native_error("decode native H2 cleanup", format!("invalid cleanup JSON: {error}"))
        })?;
        validate_binding(
            document.schema_version,
            &document.subject_id,
            &document.scenario_id,
            &document.request_sha256,
            validation.request_bytes,
            validation.subject_id,
            validation.scenario,
            "cleanup response",
        )?;
        if validation.prior_paths.contains(&document.evidence.path) {
            return Err(native_error(
                "validate native H2 cleanup",
                "cleanup and scenario evidence name the same retained artifact",
            ));
        }
        let digest = parse_sha256(&document.evidence.sha256)?;
        let total =
            validation.prior_bytes.checked_add(document.evidence.bytes).ok_or_else(|| {
                native_error("validate native H2 cleanup", "artifact byte accounting overflowed")
            })?;
        if total > validation.limits.artifact_bytes() {
            return Err(native_error(
                "validate native H2 cleanup",
                "retained artifacts exceed the aggregate byte limit",
            ));
        }
        validate_artifact(
            validation.artifact_root,
            &document.evidence.path,
            digest,
            document.evidence.bytes,
            validation.limits.artifact_bytes(),
        )?;
        CleanupObservation::new(
            validation.subject_id.to_owned(),
            document.complete,
            document.remaining_resources,
            digest,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_binding(
    schema_version: u8,
    response_subject: &str,
    response_scenario: &str,
    response_request_sha256: &str,
    request_bytes: &[u8],
    subject_id: &str,
    scenario: ScenarioSpec,
    label: &'static str,
) -> Result<(), QualificationError> {
    if schema_version != SCHEMA_VERSION {
        return Err(native_error("decode native H2 response", "unsupported schema version"));
    }
    if response_subject != subject_id || response_scenario != scenario.id().as_str() {
        return Err(native_error(
            "validate native H2 response",
            format!("{label} is bound to a different subject or scenario"),
        ));
    }
    let expected = digest_bytes(request_bytes).sha256().to_hex();
    if response_request_sha256 != expected {
        return Err(native_error(
            "validate native H2 response",
            format!("{label} is not bound to the exact request document"),
        ));
    }
    Ok(())
}
