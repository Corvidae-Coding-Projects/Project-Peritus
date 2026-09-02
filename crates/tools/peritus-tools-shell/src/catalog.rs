//! Exact deterministic shell tool descriptors.

use peritus_policy::{OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedText, ControlSet, IdempotencySemantics, ImplementationIdentity, LeaseRequirement,
    ProtocolCompatibility, Schema, SchemaProperty, SemanticVersion, SideEffectClass,
    ToolDescriptor, ToolLimits,
};
use peritus_types::CapabilityName;

use crate::{ShellError, ShellErrorKind};

const MAX_TOKEN_BYTES: u32 = 64 * 1_024;
const MAX_ARGUMENTS: u32 = 4_096;

/// Builds the canonical `shell.exec` structured-argv descriptor.
///
/// # Errors
/// Returns a typed error only if an internal descriptor constant violates the protocol contract.
pub fn exec_descriptor() -> Result<ToolDescriptor, ShellError> {
    descriptor(
        "shell.exec",
        exec_schema()?,
        vec![RiskClass::Execution],
        "peritus-tools-shell/shell.exec/v1",
        "Execute literal structured argv through authorized C2 process ownership. Production callers select either an admitted restricted native sandbox or an explicit raw-effect boundary.",
    )
}

/// Builds the canonical higher-risk `shell.script` descriptor.
///
/// # Errors
/// Returns a typed error only if an internal descriptor constant violates the protocol contract.
pub fn script_descriptor() -> Result<ToolDescriptor, ShellError> {
    descriptor(
        "shell.script",
        script_schema()?,
        vec![RiskClass::Execution, RiskClass::ExternalSideEffect],
        "peritus-tools-shell/shell.script/v1",
        "Execute explicit interpreter and script text through authorized C2 process ownership. Production callers select either an admitted restricted native sandbox or an explicit raw-effect boundary.",
    )
}

fn exec_schema() -> Result<Schema, ShellError> {
    let arguments = Schema::array(Schema::string(0, MAX_TOKEN_BYTES)?, 0, MAX_ARGUMENTS)?;
    Ok(Schema::object(
        vec![
            SchemaProperty::new("arguments".into(), arguments, true)?,
            SchemaProperty::new("executable".into(), Schema::string(1, 4_096)?, true)?,
        ],
        false,
    )?)
}

fn script_schema() -> Result<Schema, ShellError> {
    let arguments = Schema::array(Schema::string(0, MAX_TOKEN_BYTES)?, 0, MAX_ARGUMENTS)?;
    Ok(Schema::object(
        vec![
            SchemaProperty::new("arguments".into(), arguments.clone(), true)?,
            SchemaProperty::new("interpreter".into(), Schema::string(1, 4_096)?, true)?,
            SchemaProperty::new("interpreter_arguments".into(), arguments, true)?,
            SchemaProperty::new("script".into(), Schema::string(1, 256 * 1_024)?, true)?,
        ],
        false,
    )?)
}

fn descriptor(
    name: &str,
    schema: Schema,
    risks: Vec<RiskClass>,
    implementation: &str,
    description: &str,
) -> Result<ToolDescriptor, ShellError> {
    let name = CapabilityName::new(name.to_owned())
        .map_err(|error| internal(format!("invalid shell capability name: {error:?}")))?;
    let risks = RiskSet::new(risks).map_err(|error| internal(format!("{error:?}")))?;
    let operation = OperationDescriptor::new(name.clone(), OperationClass::Execution, risks)
        .map_err(|error| internal(format!("{error:?}")))?;
    ToolDescriptor::new(
        name,
        SemanticVersion::new(1, 0, 0)?,
        schema,
        operation,
        SideEffectClass::Process,
        LeaseRequirement::None,
        IdempotencySemantics::ReportPriorOutcome,
        ImplementationIdentity::new(implementation.to_owned())?,
        ToolLimits::new(600_000, 8 * 1_024 * 1_024, 16_384, 16_384, 4_096, 3, 65_536)?,
        ControlSet::new(true, true, true, true, true),
        ProtocolCompatibility::V1,
        BoundedText::new(description.to_owned())?,
    )
    .map_err(Into::into)
}

fn internal(detail: impl Into<String>) -> ShellError {
    ShellError::new(ShellErrorKind::Protocol, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_are_stable_and_distinct() {
        let exec_a = exec_descriptor().expect("exec descriptor");
        let exec_b = exec_descriptor().expect("exec descriptor");
        let script = script_descriptor().expect("script descriptor");
        assert_eq!(exec_a.canonical_bytes(), exec_b.canonical_bytes());
        assert_ne!(exec_a.descriptor_digest(), script.descriptor_digest());
        assert_eq!(exec_a.operation().operation_class(), OperationClass::Execution);
        assert_eq!(script.operation().operation_class(), OperationClass::Execution);
    }
}
