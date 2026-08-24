//! Canonical filesystem tool descriptors.

use peritus_policy::{OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedText, ControlSet, IdempotencySemantics, ImplementationIdentity, LeaseRequirement,
    ProtocolCompatibility, SemanticVersion, SideEffectClass, ToolDescriptor, ToolLimits,
};
use peritus_types::{CapabilityName, Sha256Digest};

use crate::{
    FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass,
    schemas::{
        create_schema, discover_schema, metadata_schema, patch_schema, read_schema, remove_schema,
        replace_schema, search_schema, write_schema,
    },
};

struct DescriptorSpec {
    name: &'static str,
    description: &'static str,
    class: OperationClass,
    risk: RiskClass,
    effect: SideEffectClass,
    lease: LeaseRequirement,
    replay: IdempotencySemantics,
    schema: fn() -> Result<peritus_tool_protocol::Schema, FsToolError>,
}

const SPECS: &[DescriptorSpec] = &[
    mutation_spec("fs.create", "Create one exact authorized regular file", create_schema),
    read_spec("fs.discover", "Discover a bounded immutable workspace subtree", discover_schema),
    read_spec("fs.metadata", "Inspect exact immutable workspace entry metadata", metadata_schema),
    mutation_spec("fs.patch", "Apply one authorized atomic multi-file patch", patch_schema),
    read_spec("fs.read", "Read one bounded immutable regular file", read_schema),
    mutation_spec("fs.remove", "Remove one exact authorized regular file", remove_schema),
    mutation_spec("fs.replace", "Replace one exact authorized regular file", replace_schema),
    read_spec("fs.search", "Search bounded immutable UTF-8 files literally", search_schema),
    mutation_spec("fs.write", "Create or replace one exact authorized regular file", write_schema),
];

const fn read_spec(
    name: &'static str,
    description: &'static str,
    schema: fn() -> Result<peritus_tool_protocol::Schema, FsToolError>,
) -> DescriptorSpec {
    DescriptorSpec {
        name,
        description,
        class: OperationClass::Inspection,
        risk: RiskClass::Read,
        effect: SideEffectClass::None,
        lease: LeaseRequirement::None,
        replay: IdempotencySemantics::ReplayTerminal,
        schema,
    }
}

const fn mutation_spec(
    name: &'static str,
    description: &'static str,
    schema: fn() -> Result<peritus_tool_protocol::Schema, FsToolError>,
) -> DescriptorSpec {
    DescriptorSpec {
        name,
        description,
        class: OperationClass::WorkspaceMutation,
        risk: RiskClass::ScopedWrite,
        effect: SideEffectClass::Workspace,
        lease: LeaseRequirement::Required,
        replay: IdempotencySemantics::ReportPriorOutcome,
        schema,
    }
}

/// Builds the canonical filesystem descriptor catalog.
///
/// # Errors
/// Returns a typed construction failure if an invariant in the frozen schema catalog is broken.
pub fn descriptor_catalog() -> Result<Vec<ToolDescriptor>, FsToolError> {
    SPECS.iter().map(build_descriptor).collect()
}

/// Computes a stable aggregate digest over the canonical descriptor catalog.
///
/// # Errors
/// Returns a typed construction failure if the frozen catalog is invalid.
pub fn descriptor_digest() -> Result<Sha256Digest, FsToolError> {
    let catalog = descriptor_catalog()?;
    let mut bytes = b"PERITUS-FS-TOOL-CATALOG-V1\0".to_vec();
    bytes.extend_from_slice(&(catalog.len() as u64).to_be_bytes());
    for descriptor in catalog {
        put_bytes(&mut bytes, &descriptor.canonical_bytes());
    }
    Ok(peritus_codec::sha256(&bytes))
}

fn build_descriptor(spec: &DescriptorSpec) -> Result<ToolDescriptor, FsToolError> {
    let name = capability(spec.name)?;
    let operation = OperationDescriptor::new(
        capability(spec.name)?,
        spec.class,
        RiskSet::new(vec![spec.risk]).map_err(|_| catalog_error())?,
    )
    .map_err(|_| catalog_error())?;
    ToolDescriptor::new(
        name,
        SemanticVersion::new(1, 0, 0).map_err(|_| catalog_error())?,
        (spec.schema)()?,
        operation,
        spec.effect,
        spec.lease,
        spec.replay,
        ImplementationIdentity::new(format!("peritus.tools.fs.{}/v1", spec.name))
            .map_err(|_| catalog_error())?,
        ToolLimits::new(30_000, 8 * 1_024 * 1_024, 16_384, 16_384, 1, 1, 1)
            .map_err(|_| catalog_error())?,
        ControlSet::NONE,
        ProtocolCompatibility::V1,
        BoundedText::new(spec.description.to_owned()).map_err(|_| catalog_error())?,
    )
    .map_err(|_| catalog_error())
}

fn capability(value: &str) -> Result<CapabilityName, FsToolError> {
    CapabilityName::new(value.to_owned()).map_err(|_| catalog_error())
}

fn put_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

const fn catalog_error() -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Protocol,
        FsToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "frozen filesystem descriptor catalog is invalid",
    )
}
