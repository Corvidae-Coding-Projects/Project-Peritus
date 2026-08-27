//! Canonical plugin manifest and declared authority/resource boundaries.

use std::path::{Component, Path};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::{ManifestDigest, PluginId, PluginVersion, SdkError, SdkErrorKind};

mod wire;

/// Current canonical plugin-manifest schema.
pub const MANIFEST_VERSION: u16 = 1;

/// Isolated execution mechanism selected by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginKind {
    /// Standalone executable using framed stdin/stdout.
    Process,
    /// WASI-compatible Wasm module using framed stdin/stdout.
    WasmComponent,
}

/// Relative artifact and literal arguments used to start a plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEntrypoint {
    artifact: String,
    arguments: Vec<String>,
}

impl PluginEntrypoint {
    /// Creates a relative artifact entrypoint with literal arguments.
    ///
    /// # Errors
    ///
    /// Rejects absolute/traversing artifacts or oversized/control-containing arguments.
    pub fn new(artifact: impl Into<String>, arguments: Vec<String>) -> Result<Self, SdkError> {
        let value = Self { artifact: artifact.into(), arguments };
        value.validate()?;
        Ok(value)
    }

    /// Borrows the relative artifact path.
    #[must_use]
    pub fn artifact(&self) -> &str {
        &self.artifact
    }

    /// Borrows literal startup arguments.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    fn validate(&self) -> Result<(), SdkError> {
        let path = Path::new(&self.artifact);
        let valid_path = !self.artifact.is_empty()
            && self.artifact.len() <= 512
            && !path.is_absolute()
            && path.components().all(|component| matches!(component, Component::Normal(_)));
        if !valid_path {
            return Err(manifest_error("entrypoint artifact must be one safe relative path"));
        }
        if self.arguments.len() > 64
            || self.arguments.iter().any(|argument| {
                argument.len() > 4096 || argument.chars().any(|character| character == '\0')
            })
        {
            return Err(manifest_error("entrypoint arguments exceed their bound"));
        }
        Ok(())
    }
}

/// Authority operation requested by an extension.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PluginOperation {
    /// Read-only inspection.
    Inspection,
    /// Scoped workspace mutation through a Peritus gateway.
    WorkspaceMutation,
    /// Bounded execution through a Peritus gateway.
    Execution,
    /// Scoped network access through a Peritus gateway.
    Network,
    /// Secret use through a Peritus broker.
    SecretUse,
    /// Externally visible side effect through a Peritus gateway.
    ExternalSideEffect,
}

/// One declared capability required or optionally used by the plugin.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDeclaration {
    name: String,
    operation: PluginOperation,
    required: bool,
}

impl CapabilityDeclaration {
    /// Creates a canonical capability declaration.
    ///
    /// # Errors
    ///
    /// Rejects an invalid hierarchical capability name.
    pub fn new(
        name: impl Into<String>,
        operation: PluginOperation,
        required: bool,
    ) -> Result<Self, SdkError> {
        let value = Self { name: name.into(), operation, required };
        validate_capability_name(&value.name)?;
        Ok(value)
    }

    /// Borrows the canonical capability name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the declared operation class.
    #[must_use]
    pub const fn operation(&self) -> PluginOperation {
        self.operation
    }

    /// Returns whether absence prevents startup.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }
}

/// Inclusive plugin protocol compatibility range.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProtocolRange {
    minimum: u16,
    maximum: u16,
}

impl ProtocolRange {
    /// Creates a nonzero inclusive range.
    ///
    /// # Errors
    ///
    /// Rejects zero or inverted ranges.
    pub fn new(minimum: u16, maximum: u16) -> Result<Self, SdkError> {
        if minimum == 0 || minimum > maximum {
            Err(SdkError::new(
                SdkErrorKind::IncompatibleProtocol,
                "validate plugin protocol range",
                "protocol range is zero or inverted",
            ))
        } else {
            Ok(Self { minimum, maximum })
        }
    }

    /// Returns the lowest supported version.
    #[must_use]
    pub const fn minimum(self) -> u16 {
        self.minimum
    }

    /// Returns the highest supported version.
    #[must_use]
    pub const fn maximum(self) -> u16 {
        self.maximum
    }

    /// Selects the greatest mutually supported protocol version.
    ///
    /// # Errors
    ///
    /// Returns an incompatible-protocol error when the ranges do not intersect.
    pub fn negotiate(self, host: Self) -> Result<u16, SdkError> {
        let minimum = if self.minimum > host.minimum { self.minimum } else { host.minimum };
        let maximum = if self.maximum < host.maximum { self.maximum } else { host.maximum };
        if minimum > maximum {
            Err(SdkError::new(
                SdkErrorKind::IncompatibleProtocol,
                "negotiate plugin protocol",
                "host and plugin protocol ranges do not intersect",
            ))
        } else {
            Ok(maximum)
        }
    }
}

/// Hard ceilings requested by a manifest and narrowed by host policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginQuotas {
    /// Maximum concurrent requests.
    pub concurrent_requests: u16,
    /// Maximum request or response frame size.
    pub frame_bytes: u32,
    /// Maximum result bytes over one invocation.
    pub output_bytes: u64,
    /// Maximum wall-clock duration per invocation.
    pub invocation_millis: u64,
    /// Maximum requests during one host lifecycle.
    pub lifecycle_requests: u64,
    /// Maximum protocol violations before forced termination.
    pub protocol_violations: u16,
}

impl PluginQuotas {
    /// Validates that every quota is positive.
    ///
    /// # Errors
    ///
    /// Rejects a zero quota.
    pub fn validate(self) -> Result<Self, SdkError> {
        if self.concurrent_requests == 0
            || self.frame_bytes == 0
            || self.output_bytes == 0
            || self.invocation_millis == 0
            || self.lifecycle_requests == 0
            || self.protocol_violations == 0
        {
            Err(SdkError::new(
                SdkErrorKind::LimitExceeded,
                "validate plugin quotas",
                "every plugin quota must be positive",
            ))
        } else {
            Ok(self)
        }
    }

    /// Intersects requested quotas with host ceilings.
    #[must_use]
    pub const fn narrow(self, ceiling: Self) -> Self {
        Self {
            concurrent_requests: min_u16(self.concurrent_requests, ceiling.concurrent_requests),
            frame_bytes: min_u32(self.frame_bytes, ceiling.frame_bytes),
            output_bytes: min_u64(self.output_bytes, ceiling.output_bytes),
            invocation_millis: min_u64(self.invocation_millis, ceiling.invocation_millis),
            lifecycle_requests: min_u64(self.lifecycle_requests, ceiling.lifecycle_requests),
            protocol_violations: min_u16(self.protocol_violations, ceiling.protocol_violations),
        }
    }
}

/// Detached signature metadata interpreted by the configured host trust verifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SignatureDeclaration {
    key_id: String,
    algorithm: String,
    signature: String,
}

impl SignatureDeclaration {
    /// Borrows the stable signer key identifier.
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Borrows the declared signature algorithm.
    #[must_use]
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// Borrows the encoded detached signature.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    fn validate(&self) -> Result<(), SdkError> {
        if self.key_id.is_empty()
            || self.key_id.len() > 128
            || self.algorithm.is_empty()
            || self.algorithm.len() > 64
            || self.signature.is_empty()
            || self.signature.len() > 4096
            || self.key_id.chars().any(char::is_control)
            || self.algorithm.chars().any(char::is_control)
            || self.signature.chars().any(char::is_whitespace)
        {
            Err(manifest_error("signature declaration is invalid or oversized"))
        } else {
            Ok(())
        }
    }
}

/// Exact bytes used by a trust verifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustMaterial {
    canonical_manifest: Vec<u8>,
    artifact_sha256: [u8; 32],
}

impl TrustMaterial {
    /// Borrows canonical unsigned manifest bytes.
    #[must_use]
    pub fn canonical_manifest(&self) -> &[u8] {
        &self.canonical_manifest
    }

    /// Returns the plugin artifact digest.
    #[must_use]
    pub const fn artifact_sha256(&self) -> [u8; 32] {
        self.artifact_sha256
    }

    /// Produces the unambiguous signature preimage.
    #[must_use]
    pub fn signature_preimage(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24 + self.canonical_manifest.len() + 32);
        bytes.extend_from_slice(b"peritus-plugin-trust-v1\0");
        bytes.extend_from_slice(&(self.canonical_manifest.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&self.canonical_manifest);
        bytes.extend_from_slice(&self.artifact_sha256);
        bytes
    }
}

/// Versioned canonical plugin manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    manifest_version: u16,
    id: PluginId,
    version: PluginVersion,
    kind: PluginKind,
    protocol: ProtocolRange,
    entrypoint: PluginEntrypoint,
    capabilities: Vec<CapabilityDeclaration>,
    quotas: PluginQuotas,
    signature: Option<SignatureDeclaration>,
}

impl PluginManifest {
    /// Parses strict TOML and validates all manifest invariants.
    ///
    /// # Errors
    ///
    /// Rejects malformed TOML, unknown fields, incompatible schema, or invalid collections.
    pub fn parse_toml(input: &str) -> Result<Self, SdkError> {
        let manifest: Self = toml::from_str(input).map_err(|error| {
            SdkError::new(SdkErrorKind::InvalidManifest, "parse plugin manifest", error.to_string())
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates a programmatically constructed or deserialized manifest.
    ///
    /// # Errors
    ///
    /// Rejects schema, entrypoint, capability, quota, or signature violations.
    pub fn validate(&self) -> Result<(), SdkError> {
        if self.manifest_version != MANIFEST_VERSION {
            return Err(manifest_error("unsupported plugin manifest version"));
        }
        self.protocol.negotiate(ProtocolRange::new(1, 1)?)?;
        self.entrypoint.validate()?;
        self.quotas.validate()?;
        if self.capabilities.len() > 256 {
            return Err(manifest_error("capability count exceeds its bound"));
        }
        for capability in &self.capabilities {
            validate_capability_name(capability.name())?;
        }
        if self.capabilities.windows(2).any(|pair| pair[0].name() == pair[1].name()) {
            return Err(SdkError::new(
                SdkErrorKind::NonCanonical,
                "validate plugin manifest",
                "capability names must be unique",
            ));
        }
        if self.capabilities.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(SdkError::new(
                SdkErrorKind::NonCanonical,
                "validate plugin manifest",
                "capabilities must be strictly ordered without duplicates",
            ));
        }
        if let Some(signature) = &self.signature {
            signature.validate()?;
        }
        Ok(())
    }

    /// Returns the manifest schema version.
    #[must_use]
    pub const fn manifest_version(&self) -> u16 {
        self.manifest_version
    }

    /// Borrows the plugin identifier.
    #[must_use]
    pub const fn id(&self) -> &PluginId {
        &self.id
    }

    /// Returns the plugin version.
    #[must_use]
    pub const fn version(&self) -> PluginVersion {
        self.version
    }

    /// Returns the isolation kind.
    #[must_use]
    pub const fn kind(&self) -> PluginKind {
        self.kind
    }

    /// Returns the supported protocol range.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolRange {
        self.protocol
    }

    /// Borrows the entrypoint.
    #[must_use]
    pub const fn entrypoint(&self) -> &PluginEntrypoint {
        &self.entrypoint
    }

    /// Borrows capability declarations in canonical order.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityDeclaration] {
        &self.capabilities
    }

    /// Returns requested quotas.
    #[must_use]
    pub const fn quotas(&self) -> PluginQuotas {
        self.quotas
    }

    /// Borrows detached signature metadata, when declared.
    #[must_use]
    pub const fn signature(&self) -> Option<&SignatureDeclaration> {
        self.signature.as_ref()
    }

    /// Serializes the complete manifest into canonical JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if validation or serialization fails.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SdkError> {
        self.validate()?;
        crate::canonical::bytes(self)
    }

    /// Hashes the complete canonical manifest.
    ///
    /// # Errors
    ///
    /// Returns an error if validation or serialization fails.
    pub fn digest(&self) -> Result<ManifestDigest, SdkError> {
        let bytes = self.canonical_bytes()?;
        Ok(ManifestDigest::new(Sha256::digest(bytes).into()))
    }

    /// Builds exact unsigned manifest plus artifact trust material.
    ///
    /// # Errors
    ///
    /// Returns an error if validation or serialization fails.
    pub fn trust_material(&self, artifact_sha256: [u8; 32]) -> Result<TrustMaterial, SdkError> {
        self.validate()?;
        let mut unsigned = self.clone();
        unsigned.signature = None;
        Ok(TrustMaterial {
            canonical_manifest: crate::canonical::bytes(&unsigned)?,
            artifact_sha256,
        })
    }
}

fn validate_capability_name(name: &str) -> Result<(), SdkError> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
        });
    if valid {
        Ok(())
    } else {
        Err(manifest_error("capability name is not canonical hierarchical ASCII"))
    }
}

const fn min_u16(left: u16, right: u16) -> u16 {
    if left < right { left } else { right }
}

const fn min_u32(left: u32, right: u32) -> u32 {
    if left < right { left } else { right }
}

const fn min_u64(left: u64, right: u64) -> u64 {
    if left < right { left } else { right }
}

fn manifest_error(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::InvalidManifest, "validate plugin manifest", detail)
}
