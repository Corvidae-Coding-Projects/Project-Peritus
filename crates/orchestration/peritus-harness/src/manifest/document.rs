//! Strict TOML schema-v1 document and checked domain conversion.

use serde::Deserialize;

use peritus_types::Sha256Digest;

use crate::domain::{
    ArtifactDigest, AuthoritySet, CompatibilityContract, ComponentDeclaration, ComponentId,
    ComponentIdentity, ComponentIntegrity, ComponentLocation, ComponentOwnership,
    ComponentRequirements, DependencyRequirement, FeatureTag, GraphEnvironment, HarnessLimitKind,
    HarnessLimits, LineageSeed, ManifestDigest, MediaType, Owner, Provenance, SchemaInterval,
    SchemaVersion, SourcePath, TargetPath,
};

use super::{
    ManifestError, ManifestErrorKind,
    tags::{RawAuthority, RawComponentKind, RawProtectionClass},
};

/// Fully parsed schema-v1 manifest whose fields already passed domain constructors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessManifest {
    exact_bytes: Vec<u8>,
    digest: ManifestDigest,
    lineage_seed: LineageSeed,
    limits: HarnessLimits,
    environment: GraphEnvironment,
    declarations: Vec<ComponentDeclaration>,
}

impl HarnessManifest {
    /// Parses strict UTF-8 TOML and invokes every checked domain constructor.
    ///
    /// # Errors
    /// Rejects oversized/non-UTF-8 bytes, unknown fields, schema drift, invalid limits, malformed
    /// digests, noncanonical features/dependencies/authority, and invalid declarations.
    pub fn parse(bytes: &[u8], compiled: HarnessLimits) -> Result<Self, ManifestError> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > compiled.max_manifest_bytes() {
            return Err(ManifestError::new(
                ManifestErrorKind::ManifestTooLarge,
                "manifest exceeds the compiled byte ceiling",
            ));
        }
        let text = core::str::from_utf8(bytes).map_err(|_| {
            ManifestError::new(ManifestErrorKind::InvalidUtf8, "manifest is not UTF-8")
        })?;
        let raw: RawManifest = toml::from_str(text).map_err(|error| {
            ManifestError::new(ManifestErrorKind::InvalidToml, error.to_string())
        })?;
        if raw.schema_version != 1 {
            return Err(ManifestError::new(
                ManifestErrorKind::UnsupportedSchema,
                "only manifest schema version 1 is supported",
            ));
        }
        let limits = raw.limits.apply(compiled)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limits.max_manifest_bytes() {
            return Err(ManifestError::new(
                ManifestErrorKind::ManifestTooLarge,
                "manifest exceeds its tightened byte ceiling",
            ));
        }
        let provider_features = feature_tags(raw.provider_features)?;
        let platform_features = feature_tags(raw.platform_features)?;
        let environment = GraphEnvironment::new(provider_features, platform_features)?;
        let declarations = raw
            .components
            .into_iter()
            .map(|component| component.checked(limits))
            .collect::<Result<Vec<_>, _>>()?;
        let digest = ManifestDigest::new(peritus_codec::sha256(bytes));
        Ok(Self {
            exact_bytes: bytes.to_vec(),
            digest,
            lineage_seed: LineageSeed::new(parse_digest(&raw.lineage_seed)?),
            limits,
            environment,
            declarations,
        })
    }

    /// Borrows the exact committed manifest bytes.
    #[must_use]
    pub fn exact_bytes(&self) -> &[u8] {
        &self.exact_bytes
    }
    /// Returns the SHA-256 digest of exact committed bytes.
    #[must_use]
    pub const fn digest(&self) -> ManifestDigest {
        self.digest
    }
    /// Returns the lineage seed used only by genesis construction.
    #[must_use]
    pub const fn lineage_seed(&self) -> LineageSeed {
        self.lineage_seed
    }
    /// Returns compiled or tightened E1 limits.
    #[must_use]
    pub const fn limits(&self) -> HarnessLimits {
        self.limits
    }
    /// Returns the supported provider/platform feature environment.
    #[must_use]
    pub const fn environment(&self) -> &GraphEnvironment {
        &self.environment
    }
    /// Returns manifest-order checked declarations.
    #[must_use]
    pub fn declarations(&self) -> &[ComponentDeclaration] {
        &self.declarations
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema_version: u16,
    lineage_seed: String,
    limits: RawLimits,
    provider_features: Vec<String>,
    platform_features: Vec<String>,
    components: Vec<RawComponent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLimits {
    manifest_bytes: Option<u64>,
    components: Option<u64>,
    dependency_edges: Option<u64>,
    dependency_fan_out: Option<u64>,
    component_bytes: Option<u64>,
    total_materialized_bytes: Option<u64>,
    revision_history: Option<u64>,
    receipt_history: Option<u64>,
    event_bytes: Option<u64>,
    state_bytes: Option<u64>,
    retained_diagnostics: Option<u64>,
}

impl RawLimits {
    fn apply(self, limits: HarnessLimits) -> Result<HarnessLimits, ManifestError> {
        let values = [
            (HarnessLimitKind::ManifestBytes, self.manifest_bytes),
            (HarnessLimitKind::Components, self.components),
            (HarnessLimitKind::DependencyEdges, self.dependency_edges),
            (HarnessLimitKind::DependencyFanOut, self.dependency_fan_out),
            (HarnessLimitKind::ComponentBytes, self.component_bytes),
            (HarnessLimitKind::TotalMaterializedBytes, self.total_materialized_bytes),
            (HarnessLimitKind::RevisionHistory, self.revision_history),
            (HarnessLimitKind::ReceiptHistory, self.receipt_history),
            (HarnessLimitKind::EventBytes, self.event_bytes),
            (HarnessLimitKind::StateBytes, self.state_bytes),
            (HarnessLimitKind::RetainedDiagnostics, self.retained_diagnostics),
        ];
        let overrides = values
            .into_iter()
            .filter_map(|(kind, value)| value.map(|value| (kind, value)))
            .collect::<Vec<_>>();
        limits.tightened(&overrides).map_err(Into::into)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawComponent {
    id: String,
    kind: RawComponentKind,
    schema_version: u32,
    source_path: String,
    target_path: String,
    media_type: String,
    byte_length: u64,
    content_sha256: String,
    owner: String,
    provenance: String,
    dependencies: Vec<RawDependency>,
    compatibility: RawCompatibility,
    declared_authority: Vec<RawAuthority>,
    protection_class: RawProtectionClass,
    executable_artifact_sha256: Option<String>,
}

impl RawComponent {
    fn checked(self, limits: HarnessLimits) -> Result<ComponentDeclaration, ManifestError> {
        let dependencies = self
            .dependencies
            .into_iter()
            .map(RawDependency::checked)
            .collect::<Result<Vec<_>, _>>()?;
        let compatibility = self.compatibility.checked()?;
        let authority =
            AuthoritySet::new(self.declared_authority.into_iter().map(Into::into).collect())?;
        ComponentDeclaration::new(
            ComponentIdentity::new(
                ComponentId::new(self.id)?,
                self.kind.into(),
                SchemaVersion::new(self.schema_version)?,
            ),
            ComponentLocation::new(
                SourcePath::new(self.source_path)?,
                TargetPath::new(self.target_path)?,
                MediaType::new(self.media_type)?,
            ),
            ComponentIntegrity::new(
                self.byte_length,
                parse_digest(&self.content_sha256)?,
                self.executable_artifact_sha256
                    .as_deref()
                    .map(parse_digest)
                    .transpose()?
                    .map(ArtifactDigest::new),
            ),
            ComponentOwnership::new(Owner::new(self.owner)?, Provenance::new(self.provenance)?),
            ComponentRequirements::new(
                dependencies,
                compatibility,
                authority,
                self.protection_class.into(),
            ),
            limits,
        )
        .map_err(Into::into)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDependency {
    component_id: String,
    required_kind: RawComponentKind,
    minimum_schema: u32,
    maximum_schema: u32,
    exact_content_sha256: Option<String>,
}

impl RawDependency {
    fn checked(self) -> Result<DependencyRequirement, ManifestError> {
        Ok(DependencyRequirement::new(
            ComponentId::new(self.component_id)?,
            self.required_kind.into(),
            interval(self.minimum_schema, self.maximum_schema)?,
            self.exact_content_sha256.as_deref().map(parse_digest).transpose()?,
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCompatibility {
    minimum_schema: u32,
    maximum_schema: u32,
    provider_features: Vec<String>,
    platform_features: Vec<String>,
}

impl RawCompatibility {
    fn checked(self) -> Result<CompatibilityContract, ManifestError> {
        CompatibilityContract::new(
            interval(self.minimum_schema, self.maximum_schema)?,
            feature_tags(self.provider_features)?,
            feature_tags(self.platform_features)?,
        )
        .map_err(Into::into)
    }
}

fn interval(minimum: u32, maximum: u32) -> Result<SchemaInterval, ManifestError> {
    SchemaInterval::new(SchemaVersion::new(minimum)?, SchemaVersion::new(maximum)?)
        .map_err(Into::into)
}

fn feature_tags(values: Vec<String>) -> Result<Vec<FeatureTag>, ManifestError> {
    values.into_iter().map(FeatureTag::new).collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn parse_digest(value: &str) -> Result<Sha256Digest, ManifestError> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(ManifestError::new(
            ManifestErrorKind::InvalidDigest,
            "digest must be exactly 64 lowercase hexadecimal bytes",
        ));
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = nibble(pair[0]) * 16 + nibble(pair[1]);
    }
    Ok(Sha256Digest::new(output))
}

const fn nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0,
    }
}
