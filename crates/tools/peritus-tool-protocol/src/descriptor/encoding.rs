//! Deterministic descriptor encoding.

use peritus_policy::{OperationClass, RiskClass};

use super::{IdempotencySemantics, LeaseRequirement, SideEffectClass, ToolDescriptor};

pub(super) fn canonical(value: &ToolDescriptor) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512 + value.schema.canonical_bytes().len());
    bytes.extend_from_slice(b"peritus.tool-descriptor.v1\0");
    push_bytes(&mut bytes, value.name.as_str().as_bytes());
    push_u16(&mut bytes, value.version.major());
    push_u16(&mut bytes, value.version.minor());
    push_u16(&mut bytes, value.version.patch());
    push_bytes(&mut bytes, &value.schema.canonical_bytes());
    bytes.extend_from_slice(value.schema_digest.as_bytes());
    bytes.push(operation_tag(value.operation.operation_class()));
    for risk in value.operation.risks().as_slice() {
        bytes.push(risk_tag(*risk));
    }
    bytes.push(0xff);
    bytes.push(side_effect_tag(value.side_effect));
    bytes.push(lease_tag(value.lease));
    bytes.push(idempotency_tag(value.idempotency));
    push_bytes(&mut bytes, value.implementation.as_str().as_bytes());
    bytes.extend_from_slice(&value.limits.timeout_millis.to_be_bytes());
    bytes.extend_from_slice(&value.limits.output_bytes.to_be_bytes());
    bytes.extend_from_slice(&value.limits.model_bytes.to_be_bytes());
    bytes.extend_from_slice(&value.limits.human_bytes.to_be_bytes());
    bytes.extend_from_slice(&value.limits.progress_events.to_be_bytes());
    bytes.extend_from_slice(&value.limits.artifacts.to_be_bytes());
    bytes.extend_from_slice(&value.limits.control_bytes.to_be_bytes());
    bytes.push(value.controls.bits());
    push_u16(&mut bytes, value.compatibility.minimum());
    push_u16(&mut bytes, value.compatibility.maximum());
    push_bytes(&mut bytes, value.description.as_str().as_bytes());
    bytes
}

fn push_bytes(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn push_u16(target: &mut Vec<u8>, value: u16) {
    target.extend_from_slice(&value.to_be_bytes());
}

const fn operation_tag(value: OperationClass) -> u8 {
    match value {
        OperationClass::Inspection => 1,
        OperationClass::WorkspaceMutation => 2,
        OperationClass::Execution => 3,
        OperationClass::Network => 4,
        OperationClass::DependencyEnvironment => 5,
        OperationClass::RepositoryHistoryMutation => 6,
        OperationClass::SecretUse => 7,
        OperationClass::ExternalSideEffect => 8,
        OperationClass::Acceptance => 9,
        OperationClass::Waiver => 10,
        OperationClass::PolicyAmendment => 11,
        OperationClass::HarnessPromotion => 12,
        OperationClass::HumanAuthority => 13,
        OperationClass::RawEffect => 14,
    }
}

const fn side_effect_tag(value: SideEffectClass) -> u8 {
    match value {
        SideEffectClass::None => 1,
        SideEffectClass::Workspace => 2,
        SideEffectClass::Process => 3,
        SideEffectClass::External => 4,
    }
}

const fn lease_tag(value: LeaseRequirement) -> u8 {
    match value {
        LeaseRequirement::None => 1,
        LeaseRequirement::Required => 2,
    }
}

const fn idempotency_tag(value: IdempotencySemantics) -> u8 {
    match value {
        IdempotencySemantics::ReplayTerminal => 1,
        IdempotencySemantics::ReportPriorOutcome => 2,
    }
}

const fn risk_tag(value: RiskClass) -> u8 {
    match value {
        RiskClass::Read => 1,
        RiskClass::ScopedWrite => 2,
        RiskClass::Execution => 3,
        RiskClass::Network => 4,
        RiskClass::DependencyEnvironment => 5,
        RiskClass::RepositoryHistoryMutation => 6,
        RiskClass::SecretUse => 7,
        RiskClass::ExternalSideEffect => 8,
        RiskClass::PolicyAuthority => 9,
        RiskClass::HarnessPromotion => 10,
    }
}
