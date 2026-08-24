//! Canonical `fs.read` production-router fixture.

use std::sync::Arc;

use peritus_policy::{
    AuthorityInstant, OperationClass, OperationDescriptor, OperationRegistry, RiskClass, RiskSet,
};
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CallLimits, ControlSet, IdempotencyKey, IdempotencySemantics,
    ImplementationIdentity, JsonLimits, LeaseRequirement, ProtocolCompatibility, Schema,
    SchemaProperty, SemanticVersion, SideEffectClass, ToolCall, ToolDescriptor, ToolLimits,
};
use peritus_tool_router::{RouterLimits, ToolRegistry, ToolRouter};

use crate::support::Ids;

pub fn router(idempotency: IdempotencySemantics) -> ToolRouter {
    let descriptor = descriptor(idempotency);
    let operation = OperationDescriptor::new(
        descriptor.name().clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).unwrap(),
    )
    .unwrap();
    let registry = ToolRegistry::new(
        vec![Arc::clone(&descriptor)],
        &OperationRegistry::new(vec![operation]).unwrap(),
    )
    .unwrap();
    ToolRouter::new(registry, RouterLimits::new(4, 16).unwrap())
}

pub fn descriptor(idempotency: IdempotencySemantics) -> Arc<ToolDescriptor> {
    let name = peritus_types::CapabilityName::new("fs.read".to_owned()).unwrap();
    let operation = OperationDescriptor::new(
        name.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).unwrap(),
    )
    .unwrap();
    let schema = Schema::object(
        vec![
            SchemaProperty::new(
                "max_bytes".to_owned(),
                Schema::integer(Some(1), Some(65_536)).unwrap(),
                true,
            )
            .unwrap(),
            SchemaProperty::new("path".to_owned(), Schema::string(1, 4_096).unwrap(), true)
                .unwrap(),
        ],
        false,
    )
    .unwrap();
    Arc::new(
        ToolDescriptor::new(
            name,
            SemanticVersion::new(1, 0, 0).unwrap(),
            schema,
            operation,
            SideEffectClass::None,
            LeaseRequirement::None,
            idempotency,
            ImplementationIdentity::new("production:fs.read:v1".to_owned()).unwrap(),
            ToolLimits::new(30_000, 65_536, 4_096, 4_096, 8, 4, 4_096).unwrap(),
            ControlSet::new(false, false, false, true, true),
            ProtocolCompatibility::V1,
            BoundedText::new("Read a bounded workspace file".to_owned()).unwrap(),
        )
        .unwrap(),
    )
}

pub fn call(ids: &Ids, arguments: &[u8], key: &str) -> ToolCall {
    let text = std::str::from_utf8(arguments).unwrap();
    ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(1, 0, 0).unwrap(),
        BoundedJson::parse(text, JsonLimits::PRODUCTION).unwrap(),
        CallLimits::new(30_000, 4_096, 4_096, 4_096, 8, 4).unwrap(),
        ids.revision,
        AuthorityInstant::new(peritus_types::Generation::first(), 30_000),
        IdempotencyKey::new(key.to_owned()).unwrap(),
    )
}
