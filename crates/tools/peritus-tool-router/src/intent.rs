//! Canonical B3 action-intent payload for one prepared tool call.

use peritus_policy::{ActorRole, OperationClass};
use peritus_protocol::ActionIntentDto;
use peritus_tool_protocol::{IdempotencySemantics, PreparedToolCall, SideEffectClass};
use peritus_types::{ActorId, EnvironmentId, ResourceId};

/// Version-one media type interpreted by the C4 router.
pub const TOOL_INTENT_MEDIA_TYPE: &str = "application/vnd.peritus.tool-intent.v1";

/// Canonical complete payload bound into a B3 action intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIntentPayload(Vec<u8>);

impl ToolIntentPayload {
    /// Builds deterministic domain-separated bytes from an exact prepared call.
    #[must_use]
    pub fn new(prepared: &PreparedToolCall) -> Self {
        let descriptor = prepared.descriptor();
        let call = prepared.call();
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(b"peritus.tool-intent.v1\0");
        bytes.extend_from_slice(call.action_id().as_bytes());
        bytes.extend_from_slice(descriptor.descriptor_digest().as_bytes());
        bytes.extend_from_slice(prepared.prepared_digest().as_bytes());
        bytes.extend_from_slice(prepared.arguments_digest().as_bytes());
        bytes.push(operation_tag(descriptor.operation().operation_class()));
        bytes.extend_from_slice(&call.limits().timeout_millis().to_be_bytes());
        bytes.extend_from_slice(&call.limits().output_bytes().to_be_bytes());
        bytes.extend_from_slice(&call.limits().model_bytes().to_be_bytes());
        bytes.extend_from_slice(&call.limits().human_bytes().to_be_bytes());
        bytes.extend_from_slice(&call.limits().progress_events().to_be_bytes());
        bytes.extend_from_slice(&call.limits().artifacts().to_be_bytes());
        bytes.push(effect_tag(descriptor.side_effect()));
        bytes.push(idempotency_tag(descriptor.idempotency()));
        Self(bytes)
    }

    /// Borrows canonical payload bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes into canonical payload bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

/// Constructs the exact B3 action intent that E0 must authorize and commit.
#[must_use]
pub fn tool_action_intent(
    prepared: &PreparedToolCall,
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
    resource_id: ResourceId,
) -> ActionIntentDto {
    ActionIntentDto {
        action_id: prepared.call().action_id(),
        actor_id,
        role,
        environment_id,
        resource_id,
        capability_name: prepared.descriptor().name().clone(),
        operation_class: prepared.descriptor().operation().operation_class(),
        media_type: TOOL_INTENT_MEDIA_TYPE.to_owned(),
        payload: ToolIntentPayload::new(prepared).into_bytes(),
    }
}

const fn effect_tag(effect: SideEffectClass) -> u8 {
    match effect {
        SideEffectClass::None => 1,
        SideEffectClass::Workspace => 2,
        SideEffectClass::Process => 3,
        SideEffectClass::External => 4,
    }
}
const fn idempotency_tag(value: IdempotencySemantics) -> u8 {
    match value {
        IdempotencySemantics::ReplayTerminal => 1,
        IdempotencySemantics::ReportPriorOutcome => 2,
    }
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
