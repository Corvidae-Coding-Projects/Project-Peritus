//! Platform-neutral backend descriptors and preparation trait.

use crate::{BackendAdmission, CheckedSandboxPlan, FeatureSet, SandboxError};
use peritus_types::Sha256Digest;

const MAX_IDENTITY_BYTES: usize = 128;

/// Validated stable backend name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendName(String);

impl BackendName {
    /// Validates an ASCII backend name.
    ///
    /// # Errors
    /// Rejects empty, oversized, or non-identifier names.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let value = value.into();
        if !valid_identity(&value) {
            return Err(crate::error::invalid("invalid backend name"));
        }
        Ok(Self(value))
    }
    /// Returns the name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated stable backend version.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendVersion(String);

impl BackendVersion {
    /// Validates an ASCII backend version.
    ///
    /// # Errors
    /// Rejects empty, oversized, or non-identifier versions.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let value = value.into();
        if !valid_identity(&value) {
            return Err(crate::error::invalid("invalid backend version"));
        }
        Ok(Self(value))
    }
    /// Returns the version.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTITY_BYTES
        && value.is_ascii()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

/// Backend implementation category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendKind {
    /// Backend prepares native operating-system enforcement.
    Native,
    /// Backend executes only the deterministic reference semantics.
    ReferenceOnly,
}

impl BackendKind {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Native => 0,
            Self::ReferenceOnly => 1,
        }
    }
}

/// Path interpretation implemented by a backend.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PathSemantics {
    /// Logical normalized UTF-8 paths used by the reference backend.
    LogicalUtf8,
    /// Native Unix path resolution.
    UnixNative,
    /// Native Windows path resolution.
    WindowsNative,
}

impl PathSemantics {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::LogicalUtf8 => 0,
            Self::UnixNative => 1,
            Self::WindowsNative => 2,
        }
    }
}

/// Quality of resource-limit enforcement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceFidelity {
    /// Kernel or equivalent hard enforcement.
    Hard,
    /// Supervisor measurement and termination.
    Supervisor,
    /// Exact deterministic reference accounting without OS effects.
    Reference,
}

impl ResourceFidelity {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Hard => 0,
            Self::Supervisor => 1,
            Self::Reference => 2,
        }
    }
}

/// Immutable backend capability declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDescriptor {
    name: BackendName,
    version: BackendVersion,
    kind: BackendKind,
    path_semantics: PathSemantics,
    resource_fidelity: ResourceFidelity,
    supported_features: FeatureSet,
    canonical_bytes: Vec<u8>,
    digest: Sha256Digest,
    support_digest: Sha256Digest,
}

impl BackendDescriptor {
    /// Creates and digests an immutable descriptor.
    #[must_use]
    pub fn new(
        name: BackendName,
        version: BackendVersion,
        kind: BackendKind,
        path_semantics: PathSemantics,
        resource_fidelity: ResourceFidelity,
        supported_features: FeatureSet,
    ) -> Self {
        let support_digest =
            crate::canonical::support_digest(supported_features, path_semantics, resource_fidelity);
        let mut descriptor = Self {
            name,
            version,
            kind,
            path_semantics,
            resource_fidelity,
            supported_features,
            canonical_bytes: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
            support_digest,
        };
        descriptor.canonical_bytes = crate::canonical::descriptor_bytes(&descriptor);
        descriptor.digest = peritus_codec::sha256(&descriptor.canonical_bytes);
        descriptor
    }
    /// Returns stable name.
    #[must_use]
    pub const fn name(&self) -> &BackendName {
        &self.name
    }
    /// Returns stable version.
    #[must_use]
    pub const fn version(&self) -> &BackendVersion {
        &self.version
    }
    /// Returns category.
    #[must_use]
    pub const fn kind(&self) -> BackendKind {
        self.kind
    }
    /// Returns path semantics.
    #[must_use]
    pub const fn path_semantics(&self) -> PathSemantics {
        self.path_semantics
    }
    /// Returns resource fidelity.
    #[must_use]
    pub const fn resource_fidelity(&self) -> ResourceFidelity {
        self.resource_fidelity
    }
    /// Returns supported feature set.
    #[must_use]
    pub const fn supported_features(&self) -> FeatureSet {
        self.supported_features
    }
    /// Returns the complete versioned canonical descriptor representation.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Returns complete descriptor digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns the enforcement-support digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.support_digest
    }
}

/// Backend preparation consumes only a checked plan and matching admission.
pub trait SandboxPreparation {
    /// Backend-specific prepared session type.
    type Prepared;

    /// Returns the descriptor used for admission.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Prepares an inert or native backend session.
    ///
    /// # Errors
    /// Returns a stable typed error when identities disagree or preparation fails.
    fn prepare(
        &self,
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<Self::Prepared, SandboxError>;
}
