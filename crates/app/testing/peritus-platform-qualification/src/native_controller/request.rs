//! Strict decoding and independent request binding for one controller invocation.

use std::fs;

use serde::Deserialize;

use crate::{Architecture, PackageManifest, Platform, digest_bytes, digest_file};

use super::args::ControllerPaths;

const MAX_REQUEST_BYTES: u64 = 512 * 1024;
const MAX_CONTROLLER_BYTES: u64 = 256 * 1024 * 1024;

pub(super) struct BoundRequest {
    pub(super) document: RequestDocument,
    pub(super) manifest: PackageManifest,
}

impl BoundRequest {
    pub(super) fn load_and_validate(
        paths: &ControllerPaths,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        validate_paths(paths)?;
        let metadata = fs::symlink_metadata(&paths.request)?;
        if !metadata.file_type().is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_REQUEST_BYTES
        {
            return Err("H2 request must be a nonempty bounded regular file".into());
        }
        let bytes = fs::read(&paths.request)?;
        let observed_request = digest_bytes(&bytes).sha256().to_hex();
        if observed_request != paths.request_sha256 || !is_lower_sha256(&paths.request_sha256) {
            return Err("H2 request digest differs from the exact invocation binding".into());
        }
        let document: RequestDocument = serde_json::from_slice(&bytes)?;
        if document.schema_version != 1 || document.subject_id != paths.subject_id {
            return Err("H2 request schema or fresh-subject identity differs".into());
        }
        document.validate_target()?;
        document.validate_limits()?;
        let manifest = PackageManifest::parse(document.release.manifest_toml.as_bytes())?;
        if manifest.digest().to_hex() != document.release.manifest_sha256
            || manifest.layout_digest().to_hex() != document.release.layout_sha256
            || manifest.release().as_str() != document.release.version
            || manifest.platform() != document.target.platform_value()?
            || manifest.architecture() != document.target.architecture_value()?
        {
            return Err("H2 request release fields do not bind the exact package manifest".into());
        }
        let current = std::env::current_exe()?;
        let controller = digest_file(current, MAX_CONTROLLER_BYTES)?.sha256().to_hex();
        if controller != document.controller_sha256 || !is_lower_sha256(&document.controller_sha256)
        {
            return Err("running H2 controller differs from the reviewed staged identity".into());
        }
        Ok(Self { document, manifest })
    }

    pub(super) fn scenario_id(&self) -> &str {
        &self.document.scenario.id
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RequestDocument {
    schema_version: u8,
    subject_id: String,
    pub(super) scenario: ScenarioDocument,
    target: TargetDocument,
    release: ReleaseDocument,
    controller_sha256: String,
    limits: LimitsDocument,
}

impl RequestDocument {
    pub(super) const fn target(&self) -> &TargetDocument {
        &self.target
    }

    pub(super) const fn release(&self) -> &ReleaseDocument {
        &self.release
    }

    pub(super) const fn limits(&self) -> &LimitsDocument {
        &self.limits
    }

    fn validate_target(&self) -> Result<(), Box<dyn std::error::Error>> {
        let host_platform = if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            return Err("H2 controller is running on an unsupported host".into());
        };
        let host_architecture = match std::env::consts::ARCH {
            "x86_64" => Architecture::X86_64,
            "aarch64" => Architecture::Aarch64,
            _ => return Err("H2 controller is running on an unsupported architecture".into()),
        };
        if self.target.platform_value()? != host_platform
            || self.target.architecture_value()? != host_architecture
        {
            return Err("H2 request target differs from the native controller host".into());
        }
        if self.target.version.major == 0 {
            return Err("H2 target version is not a concrete native version".into());
        }
        if self.scenario.id.is_empty()
            || self.scenario.category.is_empty()
            || self.scenario.description.is_empty()
            || !self.scenario.required
        {
            return Err("H2 scenario contract is incomplete or not required".into());
        }
        Ok(())
    }

    fn validate_limits(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.limits.duration_millis == 0
            || self.limits.output_bytes == 0
            || self.limits.response_bytes == 0
            || self.limits.artifact_bytes == 0
            || self.limits.package_artifact_bytes == 0
            || self.limits.processes == 0
            || self.limits.response_bytes > self.limits.output_bytes
        {
            return Err("H2 request contains invalid controller limits".into());
        }
        Ok(())
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScenarioDocument {
    pub(super) id: String,
    pub(super) category: String,
    pub(super) required: bool,
    pub(super) description: String,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetDocument {
    platform: String,
    architecture: String,
    version: VersionDocument,
}

impl TargetDocument {
    pub(super) fn platform_name(&self) -> &str {
        &self.platform
    }

    fn platform_value(&self) -> Result<Platform, Box<dyn std::error::Error>> {
        match self.platform.as_str() {
            "linux" => Ok(Platform::Linux),
            "macos" => Ok(Platform::Macos),
            "windows" => Ok(Platform::Windows),
            _ => Err("H2 target platform is unknown".into()),
        }
    }

    fn architecture_value(&self) -> Result<Architecture, Box<dyn std::error::Error>> {
        match self.architecture.as_str() {
            "x86_64" => Ok(Architecture::X86_64),
            "aarch64" => Ok(Architecture::Aarch64),
            _ => Err("H2 target architecture is unknown".into()),
        }
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VersionDocument {
    pub(super) major: u16,
    pub(super) minor: u16,
    pub(super) patch: u16,
    pub(super) build: u32,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReleaseDocument {
    version: String,
    manifest_sha256: String,
    layout_sha256: String,
    manifest_toml: String,
}

impl ReleaseDocument {
    pub(super) fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LimitsDocument {
    duration_millis: u64,
    output_bytes: u64,
    response_bytes: u64,
    artifact_bytes: u64,
    package_artifact_bytes: u64,
    processes: u32,
}

impl LimitsDocument {
    pub(super) const fn artifact_bytes(&self) -> u64 {
        self.artifact_bytes
    }
}

fn validate_paths(paths: &ControllerPaths) -> Result<(), Box<dyn std::error::Error>> {
    let subject = fs::canonicalize(&paths.subject_root)?;
    let package = fs::canonicalize(&paths.package_root)?;
    let artifacts = fs::canonicalize(&paths.artifact_root)?;
    let request = fs::canonicalize(&paths.request)?;
    if !package.starts_with(&subject)
        || !request.starts_with(&subject)
        || artifacts.starts_with(&subject)
        || paths.response.exists()
        || paths.cleanup_response.exists()
    {
        return Err("H2 controller paths violate fresh-subject ownership".into());
    }
    Ok(())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
