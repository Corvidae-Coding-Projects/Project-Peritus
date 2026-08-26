//! Closed component catalog and strictly validated declarations.

use peritus_types::Sha256Digest;

use crate::domain::{
    ArtifactDigest, AuthoritySet, CompatibilityContract, ComponentId, ComponentKind,
    DependencyRequirement, HarnessDomainError, HarnessDomainErrorKind, HarnessLimits, MediaType,
    Owner, ProtectionClass, Provenance, SchemaVersion, SourcePath, TargetPath,
};

/// Stable identity and schema of one component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentIdentity {
    id: ComponentId,
    kind: ComponentKind,
    schema_version: SchemaVersion,
}

impl ComponentIdentity {
    /// Constructs an identity from already checked nominal values.
    #[must_use]
    pub const fn new(id: ComponentId, kind: ComponentKind, schema_version: SchemaVersion) -> Self {
        Self { id, kind, schema_version }
    }
}

/// Source, target, and media binding for one component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentLocation {
    source_path: SourcePath,
    target_path: TargetPath,
    media_type: MediaType,
}

impl ComponentLocation {
    /// Constructs an exact checked location binding.
    #[must_use]
    pub const fn new(
        source_path: SourcePath,
        target_path: TargetPath,
        media_type: MediaType,
    ) -> Self {
        Self { source_path, target_path, media_type }
    }
}

/// Byte and digest binding for one component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentIntegrity {
    byte_length: u64,
    content_digest: Sha256Digest,
    executable_artifact_digest: Option<ArtifactDigest>,
}

impl ComponentIntegrity {
    /// Constructs an exact integrity binding.
    #[must_use]
    pub const fn new(
        byte_length: u64,
        content_digest: Sha256Digest,
        executable_artifact_digest: Option<ArtifactDigest>,
    ) -> Self {
        Self { byte_length, content_digest, executable_artifact_digest }
    }
}

/// Human ownership and provenance binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentOwnership {
    owner: Owner,
    provenance: Provenance,
}

impl ComponentOwnership {
    /// Constructs ownership from validated text.
    #[must_use]
    pub const fn new(owner: Owner, provenance: Provenance) -> Self {
        Self { owner, provenance }
    }
}

/// Dependency, compatibility, authority, and protection declarations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentRequirements {
    dependencies: Vec<DependencyRequirement>,
    compatibility: CompatibilityContract,
    declared_authority: AuthoritySet,
    protection_class: ProtectionClass,
}

impl ComponentRequirements {
    /// Constructs requirement fields; the complete declaration validates them against its kind.
    #[must_use]
    pub const fn new(
        dependencies: Vec<DependencyRequirement>,
        compatibility: CompatibilityContract,
        declared_authority: AuthoritySet,
        protection_class: ProtectionClass,
    ) -> Self {
        Self { dependencies, compatibility, declared_authority, protection_class }
    }
}

/// Complete immutable checked declaration of one harness component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDeclaration {
    identity: ComponentIdentity,
    location: ComponentLocation,
    integrity: ComponentIntegrity,
    ownership: ComponentOwnership,
    requirements: ComponentRequirements,
}

impl ComponentDeclaration {
    /// Validates the complete declaration against limits and compiled policy.
    ///
    /// # Errors
    ///
    /// Rejects oversized content or dependency sets, noncanonical dependencies, self-edges,
    /// protection or compatibility mismatches, and authority above the kind ceiling.
    pub fn new(
        identity: ComponentIdentity,
        location: ComponentLocation,
        integrity: ComponentIntegrity,
        ownership: ComponentOwnership,
        requirements: ComponentRequirements,
        limits: HarnessLimits,
    ) -> Result<Self, HarnessDomainError> {
        let id = identity.id.clone();
        if integrity.byte_length > limits.max_component_bytes() {
            return Err(HarnessDomainError::component_numbers(
                HarnessDomainErrorKind::ComponentTooLarge,
                id,
                limits.max_component_bytes(),
                integrity.byte_length,
            ));
        }
        let dependency_count = u64::try_from(requirements.dependencies.len()).map_err(|_| {
            HarnessDomainError::component(HarnessDomainErrorKind::ArithmeticOverflow, id.clone())
        })?;
        if dependency_count > limits.max_dependency_fan_out() {
            return Err(HarnessDomainError::component_numbers(
                HarnessDomainErrorKind::TooManyDependencies,
                id,
                limits.max_dependency_fan_out(),
                dependency_count,
            ));
        }
        for pair in requirements.dependencies.windows(2) {
            if pair[0].component_id() >= pair[1].component_id() {
                let kind = if pair[0].component_id() == pair[1].component_id() {
                    HarnessDomainErrorKind::DuplicateDependency
                } else {
                    HarnessDomainErrorKind::NonCanonicalOrder
                };
                return Err(HarnessDomainError::components(
                    kind,
                    identity.id,
                    pair[1].component_id().clone(),
                ));
            }
        }
        if requirements
            .dependencies
            .iter()
            .any(|dependency| dependency.component_id() == &identity.id)
        {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::SelfDependency,
                identity.id,
            ));
        }
        if requirements.protection_class != identity.kind.protection_class() {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::ProtectionMismatch,
                identity.id,
            ));
        }
        if !requirements.compatibility.supported_schema().contains(identity.schema_version) {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::CompatibilityMismatch,
                identity.id,
            ));
        }
        if !requirements.declared_authority.is_subset_of(identity.kind.authority_ceiling()) {
            return Err(HarnessDomainError::component(
                HarnessDomainErrorKind::AuthorityExceeded,
                identity.id,
            ));
        }
        Ok(Self { identity, location, integrity, ownership, requirements })
    }

    /// Returns the stable component identity.
    #[must_use]
    pub const fn id(&self) -> &ComponentId {
        &self.identity.id
    }
    /// Returns the closed component kind.
    #[must_use]
    pub const fn kind(&self) -> ComponentKind {
        self.identity.kind
    }
    /// Returns the nonzero schema version.
    #[must_use]
    pub const fn schema_version(&self) -> SchemaVersion {
        self.identity.schema_version
    }
    /// Returns the source path below the harness component root.
    #[must_use]
    pub const fn source_path(&self) -> &SourcePath {
        &self.location.source_path
    }
    /// Returns the C1-relative materialization target.
    #[must_use]
    pub const fn target_path(&self) -> &TargetPath {
        &self.location.target_path
    }
    /// Returns the declared media type.
    #[must_use]
    pub const fn media_type(&self) -> &MediaType {
        &self.location.media_type
    }
    /// Returns the exact component byte count.
    #[must_use]
    pub const fn byte_length(&self) -> u64 {
        self.integrity.byte_length
    }
    /// Returns the exact component SHA-256 digest.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.integrity.content_digest
    }
    /// Returns the independently bound executable artifact digest.
    #[must_use]
    pub const fn executable_artifact_digest(&self) -> Option<ArtifactDigest> {
        self.integrity.executable_artifact_digest
    }
    /// Returns the component owner.
    #[must_use]
    pub const fn owner(&self) -> &Owner {
        &self.ownership.owner
    }
    /// Returns the exact provenance statement.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.ownership.provenance
    }
    /// Borrows ordered dependency requirements.
    #[must_use]
    pub fn dependencies(&self) -> &[DependencyRequirement] {
        &self.requirements.dependencies
    }
    /// Returns the complete compatibility contract.
    #[must_use]
    pub const fn compatibility(&self) -> &CompatibilityContract {
        &self.requirements.compatibility
    }
    /// Returns the descriptive authority declaration.
    #[must_use]
    pub const fn declared_authority(&self) -> AuthoritySet {
        self.requirements.declared_authority
    }
    /// Returns the compiled protection class.
    #[must_use]
    pub const fn protection_class(&self) -> ProtectionClass {
        self.requirements.protection_class
    }
}

/// Canonical artifact roots retained by a checked graph and every revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactRoot {
    component_id: ComponentId,
    content_digest: Sha256Digest,
    executable_artifact_digest: Option<ArtifactDigest>,
}

impl ArtifactRoot {
    pub(crate) fn from_declaration(declaration: &ComponentDeclaration) -> Self {
        Self {
            component_id: declaration.identity.id.clone(),
            content_digest: declaration.integrity.content_digest,
            executable_artifact_digest: declaration.integrity.executable_artifact_digest,
        }
    }

    /// Returns the component owning this root.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }
    /// Returns the source-content artifact digest.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    /// Returns the independently bound executable artifact digest.
    #[must_use]
    pub const fn executable_artifact_digest(&self) -> Option<ArtifactDigest> {
        self.executable_artifact_digest
    }
}
