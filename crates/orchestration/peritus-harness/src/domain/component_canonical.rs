//! Canonical declaration encoding and checked reconstruction.

use crate::domain::{
    ArtifactDigest, AuthoritySet, CanonicalEncoder, CanonicalReader, CompatibilityContract,
    ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
    ComponentLocation, ComponentOwnership, ComponentRequirements, DependencyRequirement,
    FeatureTag, HarnessDomainError, HarnessDomainErrorKind, HarnessLimitKind, HarnessLimits,
    MediaType, Owner, ProtectionClass, Provenance, SchemaInterval, SchemaVersion, SourcePath,
    TargetPath,
};

impl ComponentDeclaration {
    pub(crate) fn encode(&self, encoder: &mut CanonicalEncoder) {
        encoder.string(self.id().as_str());
        encoder.u8(self.kind().tag());
        encoder.u32(self.schema_version().get());
        encoder.string(self.source_path().as_str());
        encoder.string(self.target_path().as_str());
        encoder.string(self.media_type().as_str());
        encoder.u64(self.byte_length());
        encoder.digest(self.content_digest());
        encoder.string(self.owner().as_str());
        encoder.string(self.provenance().as_str());
        encoder.len(self.dependencies().len());
        for dependency in self.dependencies() {
            encoder.string(dependency.component_id().as_str());
            encoder.u8(dependency.required_kind().tag());
            encoder.u32(dependency.compatible_schema().minimum().get());
            encoder.u32(dependency.compatible_schema().maximum().get());
            encoder.optional_digest(dependency.exact_content_digest());
        }
        encode_compatibility(encoder, self.compatibility());
        encoder.u16(self.declared_authority().bits());
        encoder.u8(self.protection_class().tag());
        encoder.optional_digest(self.executable_artifact_digest().map(ArtifactDigest::digest));
    }

    pub(crate) fn decode(
        reader: &mut CanonicalReader<'_>,
        limits: HarnessLimits,
    ) -> Result<Self, HarnessDomainError> {
        let identity = ComponentIdentity::new(
            ComponentId::new(reader.string()?)?,
            ComponentKind::from_tag(reader.u8()?)?,
            SchemaVersion::new(reader.u32()?)?,
        );
        let location = ComponentLocation::new(
            SourcePath::new(reader.string()?)?,
            TargetPath::new(reader.string()?)?,
            MediaType::new(reader.string()?)?,
        );
        let byte_length = reader.u64()?;
        let content_digest = reader.digest()?;
        let ownership = ComponentOwnership::new(
            Owner::new(reader.string()?)?,
            Provenance::new(reader.string()?)?,
        );
        let dependency_count = reader.length()?;
        if u64::try_from(dependency_count).unwrap_or(u64::MAX) > limits.max_dependency_fan_out() {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::TooManyDependencies,
                HarnessLimitKind::DependencyFanOut,
                limits.max_dependency_fan_out(),
                u64::try_from(dependency_count).unwrap_or(u64::MAX),
            ));
        }
        let mut dependencies = Vec::with_capacity(dependency_count);
        for _ in 0..dependency_count {
            dependencies.push(decode_dependency(reader)?);
        }
        let compatibility = decode_compatibility(reader, limits)?;
        let authority = AuthoritySet::from_canonical_bits(reader.u16()?)?;
        let protection = ProtectionClass::from_tag(reader.u8()?)?;
        let executable = reader.optional_digest()?.map(ArtifactDigest::new);
        Self::new(
            identity,
            location,
            ComponentIntegrity::new(byte_length, content_digest, executable),
            ownership,
            ComponentRequirements::new(dependencies, compatibility, authority, protection),
            limits,
        )
    }
}

fn decode_dependency(
    reader: &mut CanonicalReader<'_>,
) -> Result<DependencyRequirement, HarnessDomainError> {
    let component_id = ComponentId::new(reader.string()?)?;
    let required_kind = ComponentKind::from_tag(reader.u8()?)?;
    let interval = SchemaInterval::new(
        SchemaVersion::new(reader.u32()?)?,
        SchemaVersion::new(reader.u32()?)?,
    )?;
    Ok(DependencyRequirement::new(component_id, required_kind, interval, reader.optional_digest()?))
}

fn encode_compatibility(encoder: &mut CanonicalEncoder, contract: &CompatibilityContract) {
    encoder.u32(contract.supported_schema().minimum().get());
    encoder.u32(contract.supported_schema().maximum().get());
    encoder.len(contract.provider_features().len());
    for feature in contract.provider_features() {
        encoder.string(feature.as_str());
    }
    encoder.len(contract.platform_features().len());
    for feature in contract.platform_features() {
        encoder.string(feature.as_str());
    }
}

fn decode_compatibility(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<CompatibilityContract, HarnessDomainError> {
    let interval = SchemaInterval::new(
        SchemaVersion::new(reader.u32()?)?,
        SchemaVersion::new(reader.u32()?)?,
    )?;
    CompatibilityContract::new(
        interval,
        decode_features(reader, limits)?,
        decode_features(reader, limits)?,
    )
}

fn decode_features(
    reader: &mut CanonicalReader<'_>,
    limits: HarnessLimits,
) -> Result<Vec<FeatureTag>, HarnessDomainError> {
    let count = reader.length()?;
    if u64::try_from(count).unwrap_or(u64::MAX) > limits.max_dependency_edges() {
        return Err(HarnessDomainError::limit(
            HarnessDomainErrorKind::TooManyDependencyEdges,
            HarnessLimitKind::DependencyEdges,
            limits.max_dependency_edges(),
            u64::try_from(count).unwrap_or(u64::MAX),
        ));
    }
    let mut features = Vec::with_capacity(count);
    for _ in 0..count {
        features.push(FeatureTag::new(reader.string()?)?);
    }
    Ok(features)
}
