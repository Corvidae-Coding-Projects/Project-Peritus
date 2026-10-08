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
        "Create an authorized candidate with a durable adoptable result receipt",
        candidate_schema,
    ),
    read_spec(
        "git.diff",
        "Page complete immutable changed paths and exact patch byte ranges",
        diff_schema,
    ),
    read_spec(
        "git.history",
        "Page complete immutable commit and parent observations",
        history_schema,
    ),
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
        "Restore a retained snapshot with a durable adoptable result receipt",
        rollback_schema,
    ),
    read_spec("git.snapshot", "Inspect current or retained snapshot identity", snapshot_schema),
    read_spec(
        "git.status",
        "Page complete exact structured immutable Git status",
        status_schema,
    ),
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
    let mut bytes = b"PERITUS-GIT-TOOL-CATALOG-V3\0".to_vec();
    let catalog_length = u64::try_from(catalog.len()).expect("bounded Git catalog length fits u64");
    bytes.extend_from_slice(&catalog_length.to_be_bytes());
    for descriptor in catalog {
        put_bytes(&mut bytes, &descriptor.canonical_bytes());
    }
    Ok(peritus_codec::sha256(&bytes))
}

fn build_descriptor(spec: &DescriptorSpec) -> Result<ToolDescriptor, GitToolError> {
    let version = if matches!(
        spec.name,
        "git.candidate" | "git.diff" | "git.history" | "git.rollback" | "git.status"
    ) {
        2
    } else {
        1
    };
    let operation = OperationDescriptor::new(
        capability(spec.name)?,
        spec.class,
        RiskSet::new(vec![spec.risk]).map_err(|_| catalog_error())?,
    )
    .map_err(|_| catalog_error())?;
    ToolDescriptor::new(
        capability(spec.name)?,
        SemanticVersion::new(version, 0, 0).map_err(|_| catalog_error())?,
        (spec.schema)()?,
        operation,
        spec.effect,
        spec.lease,
        spec.replay,
        ImplementationIdentity::new(format!("peritus.tools.git.{}/v{version}", spec.name))
            .map_err(|_| catalog_error())?,
        ToolLimits::with_optional_timeout(None, 8 * 1_024 * 1_024, 16_384, 16_384, 1, 1, 1)
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
    let length = u64::try_from(value.len()).expect("bounded Git descriptor length fits u64");
    target.extend_from_slice(&length.to_be_bytes());
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
