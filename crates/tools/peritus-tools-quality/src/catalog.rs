//! Exact deterministic quality tool descriptors.

use peritus_policy::{OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedText, ControlSet, IdempotencySemantics, ImplementationIdentity, LeaseRequirement,
    ProtocolCompatibility, Schema, SchemaProperty, SemanticVersion, SideEffectClass,
    ToolDescriptor, ToolLimits,
};
use peritus_types::CapabilityName;

use crate::{QualityError, QualityErrorKind};

/// Builds the canonical `quality.discover` descriptor.
///
/// # Errors
/// Returns a typed error only if an internal descriptor constant violates the protocol contract.
pub fn discover_descriptor() -> Result<ToolDescriptor, QualityError> {
    descriptor(
        "quality.discover",
        Schema::object(Vec::new(), false)?,
        OperationClass::Inspection,
        vec![RiskClass::Read],
        SideEffectClass::None,
        IdempotencySemantics::ReplayTerminal,
        ControlSet::NONE,
        "peritus-tools-quality/quality.discover/v1",
        "Discover explicit and known project quality checks without asserting acceptance policy.",
    )
}

/// Builds the canonical `quality.run` descriptor.
///
/// # Errors
/// Returns a typed error only if an internal descriptor constant violates the protocol contract.
pub fn run_descriptor() -> Result<ToolDescriptor, QualityError> {
    descriptor(
        "quality.run",
        Schema::object(
            vec![SchemaProperty::new("gate".into(), Schema::string(1, 128)?, true)?],
            false,
        )?,
        OperationClass::Execution,
        vec![RiskClass::Execution],
        SideEffectClass::Process,
        IdempotencySemantics::ReportPriorOutcome,
        ControlSet::new(false, false, false, true, true),
        "peritus-tools-quality/quality.run/v1",
        "Run one exact cataloged check through C2/C3 and return candidate B2 evidence inputs.",
    )
}

#[allow(clippy::too_many_arguments)]
fn descriptor(
    name: &str,
    schema: Schema,
    operation_class: OperationClass,
    risks: Vec<RiskClass>,
    side_effect: SideEffectClass,
    idempotency: IdempotencySemantics,
    controls: ControlSet,
    implementation: &str,
    description: &str,
) -> Result<ToolDescriptor, QualityError> {
    let name = CapabilityName::new(name.to_owned())
        .map_err(|error| internal(format!("invalid quality capability name: {error:?}")))?;
    let risks = RiskSet::new(risks).map_err(|error| internal(format!("{error:?}")))?;
    let operation = OperationDescriptor::new(name.clone(), operation_class, risks)
        .map_err(|error| internal(format!("{error:?}")))?;
    ToolDescriptor::new(
        name,
        SemanticVersion::new(1, 0, 0)?,
        schema,
        operation,
        side_effect,
        LeaseRequirement::None,
        idempotency,
        ImplementationIdentity::new(implementation.to_owned())?,
        ToolLimits::new(600_000, 8 * 1_024 * 1_024, 32_768, 32_768, 4_096, 3, 65_536)?,
        controls,
        ProtocolCompatibility::V1,
        BoundedText::new(description.to_owned())?,
    )
    .map_err(Into::into)
}

fn internal(detail: impl Into<String>) -> QualityError {
    QualityError::new(QualityErrorKind::Protocol, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_refines_b1_classes_exactly() {
        let discover = discover_descriptor().expect("discover descriptor");
        let run = run_descriptor().expect("run descriptor");
        assert_eq!(discover.operation().operation_class(), OperationClass::Inspection);
        assert_eq!(run.operation().operation_class(), OperationClass::Execution);
        assert_ne!(discover.descriptor_digest(), run.descriptor_digest());
    }
}
