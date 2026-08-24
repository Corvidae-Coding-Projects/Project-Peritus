//! Production protocol/router fixtures shared by integration tests.

use std::sync::Arc;

use peritus_policy::{OperationClass, OperationDescriptor, OperationRegistry, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CallLimits, ControlSet, IdempotencyKey, IdempotencySemantics,
    ImplementationIdentity, JsonLimits, LeaseRequirement, ProtocolCompatibility, Schema,
    SchemaProperty, SemanticVersion, SideEffectClass, ToolCall, ToolDescriptor, ToolLimits,
    Truncation, TruncationMetadata,
};
use peritus_tool_router::{RouterLimits, ToolAuthorizationRequest, ToolRegistry, ToolRouter};
use peritus_types::Sha256Digest;

use super::{AuthorityReceipts, Ids};

pub fn router(idempotency: IdempotencySemantics) -> ToolRouter {
    let (descriptor, operations) = descriptor_and_operations(idempotency);
    ToolRouter::new(
        ToolRegistry::new(vec![descriptor], &operations).unwrap(),
        RouterLimits::new(2, 8).unwrap(),
    )
}

pub fn call(ids: &Ids, key: &str) -> ToolCall {
    ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(1, 0, 0).unwrap(),
        BoundedJson::parse(r#"{"count":1}"#, JsonLimits::PRODUCTION).unwrap(),
        CallLimits::new(1_000, 2_048, 256, 256, 4, 1).unwrap(),
        ids.revision,
        peritus_policy::AuthorityInstant::new(peritus_types::Generation::first(), 80),
        IdempotencyKey::new(key.to_owned()).unwrap(),
    )
}

pub const fn authority_request<'a>(
    ids: &Ids,
    intent: &'a peritus_protocol::ActionIntentDto,
    receipts: &'a AuthorityReceipts,
    prepared_digest: Sha256Digest,
) -> ToolAuthorizationRequest<'a> {
    ToolAuthorizationRequest::new(
        intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        receipts.observed_at,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        prepared_digest,
    )
}

pub const fn complete_truncation() -> TruncationMetadata {
    TruncationMetadata {
        output: Truncation::Complete,
        model: Truncation::Complete,
        human: Truncation::Complete,
    }
}

fn descriptor_and_operations(
    idempotency: IdempotencySemantics,
) -> (Arc<ToolDescriptor>, OperationRegistry) {
    let name = peritus_types::CapabilityName::new("fixture.inspect".to_owned()).unwrap();
    let operation = OperationDescriptor::new(
        name.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).unwrap(),
    )
    .unwrap();
    let authenticated = OperationDescriptor::new(
        name.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).unwrap(),
    )
    .unwrap();
    let schema = Schema::object(
        vec![
            SchemaProperty::new(
                "count".to_owned(),
                Schema::integer(Some(0), Some(9)).unwrap(),
                true,
            )
            .unwrap(),
        ],
        false,
    )
    .unwrap();
    let descriptor = ToolDescriptor::new(
        name,
        SemanticVersion::new(1, 0, 0).unwrap(),
        schema,
        operation,
        SideEffectClass::None,
        LeaseRequirement::None,
        idempotency,
        ImplementationIdentity::new("fixture-router-dispatcher".to_owned()).unwrap(),
        ToolLimits::new(2_000, 4_096, 512, 512, 8, 2, 128).unwrap(),
        ControlSet::new(false, false, false, true, true),
        ProtocolCompatibility::V1,
        BoundedText::new("fixture".to_owned()).unwrap(),
    )
    .unwrap();
    (Arc::new(descriptor), OperationRegistry::new(vec![authenticated]).unwrap())
}
