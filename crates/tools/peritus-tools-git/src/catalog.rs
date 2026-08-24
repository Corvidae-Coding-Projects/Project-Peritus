//! Canonical Git tool descriptors.

use peritus_policy::{OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedText, ControlSet, IdempotencySemantics, ImplementationIdentity, LeaseRequirement,
    ProtocolCompatibility, SemanticVersion, SideEffectClass, ToolDescriptor, ToolLimits,
};
use peritus_types::{CapabilityName, Sha256Digest};

use crate::{
    GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass,
    schemas::{
        candidate_schema, diff_schema, history_schema, merge_schema, rollback_schema,
        snapshot_schema, status_schema,
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
    schema: fn() -> Result<peritus_tool_protocol::Schema, GitToolError>,
}

const SPECS: &[DescriptorSpec] = &[
    mutation_spec(
        "git.candidate",
        "Create an authorized candidate and retained snapshot",
        candidate_schema,
    ),
    read_spec("git.diff", "Observe a bounded immutable structured Git diff", diff_schema),
    read_spec("git.history", "Observe bounded immutable structured Git history", history_schema),
    DescriptorSpec {
        name: "git.merge",
        description: "Request separately authorized branch delivery when C1 supports it",
        class: OperationClass::RepositoryHistoryMutation,
        risk: RiskClass::RepositoryHistoryMutation,
        effect: SideEffectClass::Workspace,
        lease: LeaseRequirement::Required,
        replay: IdempotencySemantics::ReportPriorOutcome,
        schema: merge_schema,
    },
    mutation_spec(
        "git.rollback",
        "Restore a retained snapshot as an authorized successor",
        rollback_schema,
    ),
    read_spec("git.snapshot", "Inspect current or retained snapshot identity", snapshot_schema),
    read_spec("git.status", "Observe exact structured immutable Git status", status_schema),
];

const fn read_spec(
    name: &'static str,
    description: &'static str,
    schema: fn() -> Result<peritus_tool_protocol::Schema, GitToolError>,
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
    schema: fn() -> Result<peritus_tool_protocol::Schema, GitToolError>,
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

/// Builds the canonical Git descriptor catalog.
///
/// # Errors
/// Returns a typed construction failure if a frozen descriptor invariant is broken.
pub fn descriptor_catalog() -> Result<Vec<ToolDescriptor>, GitToolError> {
    SPECS.iter().map(build_descriptor).collect()
}

/// Computes a stable aggregate digest over the canonical Git descriptor catalog.
///
/// # Errors
/// Returns a typed construction failure if the frozen catalog is invalid.
pub fn descriptor_digest() -> Result<Sha256Digest, GitToolError> {
    let catalog = descriptor_catalog()?;
    let mut bytes = b"PERITUS-GIT-TOOL-CATALOG-V1\0".to_vec();
    bytes.extend_from_slice(&(catalog.len() as u64).to_be_bytes());
    for descriptor in catalog {
        put_bytes(&mut bytes, &descriptor.canonical_bytes());
    }
    Ok(peritus_codec::sha256(&bytes))
}

fn build_descriptor(spec: &DescriptorSpec) -> Result<ToolDescriptor, GitToolError> {
    let operation = OperationDescriptor::new(
        capability(spec.name)?,
        spec.class,
        RiskSet::new(vec![spec.risk]).map_err(|_| catalog_error())?,
    )
    .map_err(|_| catalog_error())?;
    ToolDescriptor::new(
        capability(spec.name)?,
        SemanticVersion::new(1, 0, 0).map_err(|_| catalog_error())?,
        (spec.schema)()?,
        operation,
        spec.effect,
        spec.lease,
        spec.replay,
        ImplementationIdentity::new(format!("peritus.tools.git.{}/v1", spec.name))
            .map_err(|_| catalog_error())?,
        ToolLimits::new(30_000, 8 * 1_024 * 1_024, 16_384, 16_384, 1, 1, 1)
            .map_err(|_| catalog_error())?,
        ControlSet::NONE,
        ProtocolCompatibility::V1,
        BoundedText::new(spec.description.to_owned()).map_err(|_| catalog_error())?,
    )
    .map_err(|_| catalog_error())
}

fn capability(value: &str) -> Result<CapabilityName, GitToolError> {
    CapabilityName::new(value.to_owned()).map_err(|_| catalog_error())
}

fn put_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

const fn catalog_error() -> GitToolError {
    GitToolError::new(
        GitToolErrorKind::Protocol,
        GitToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "frozen Git descriptor catalog is invalid",
    )
}
