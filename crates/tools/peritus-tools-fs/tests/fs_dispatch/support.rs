//! Exact router and authority assembly for filesystem production-flow tests.

use std::sync::Arc;

use peritus_policy::{
    ActorRole, AuthorityInstant, OperationDescriptor, OperationRegistry, RiskSet,
};
use peritus_protocol::ActionIntentDto;
use peritus_tool_protocol::{
    BoundedJson, CallLimits, IdempotencyKey, JsonLimits, PreparedToolCall, SemanticVersion,
    ToolCall, ToolDescriptor,
};
use peritus_tool_router::{
    RouterLimits, ToolAuthorizationRequest, ToolRegistry, ToolRouter, tool_action_intent,
};
use peritus_tools_fs::{CompiledMutation, FsDispatchKind, FsDispatcher, descriptor_catalog};
use peritus_workspace::{
    MutationOutcome, WorkspaceCallerBinding, WorkspaceGateway,
    patch_authorization_payload_for_caller,
};
use tempfile::TempDir;

use super::authority_support::{self, AuthorityReceipts, Ids};

pub fn arguments(value: &str) -> BoundedJson {
    BoundedJson::parse(value, JsonLimits::PRODUCTION).expect("bounded tool arguments")
}

pub fn prepare(ids: &Ids, name: &str, arguments: BoundedJson) -> (ToolRouter, PreparedToolCall) {
    let descriptor = descriptor(name);
    let operation = OperationDescriptor::new(
        descriptor.operation().name().clone(),
        descriptor.operation().operation_class(),
        RiskSet::new(descriptor.operation().risks().as_slice().to_vec()).expect("operation risks"),
    )
    .expect("operation descriptor");
    let operations =
        OperationRegistry::new(vec![operation]).expect("authenticated operation registry");
    let registry =
        ToolRegistry::new(vec![Arc::new(descriptor)], &operations).expect("exact tool registry");
    let router = ToolRouter::new(registry, RouterLimits::new(1, 4).expect("router limits"));
    let call = ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(1, 0, 0).expect("tool version"),
        arguments,
        CallLimits::new(1_000, 64 * 1_024, 4_096, 4_096, 1, 1).expect("call limits"),
        ids.revision,
        AuthorityInstant::new(peritus_types::Generation::first(), 80),
        IdempotencyKey::new(format!("{name}-dispatch")).expect("idempotency key"),
    );
    let prepared = router.prepare(call).expect("prepared tool call");
    (router, prepared)
}

pub fn caller(prepared: &PreparedToolCall, ids: &Ids) -> WorkspaceCallerBinding {
    WorkspaceCallerBinding::new(
        ids.action,
        ids.actor,
        ActorRole::Writer,
        ids.workspace,
        ids.environment,
        ids.resource,
        prepared.descriptor().name().clone(),
        prepared.descriptor_digest().get(),
        prepared.prepared_digest(),
    )
}

#[allow(clippy::too_many_arguments, reason = "the fixture binds two authorities and one effect")]
pub fn dispatch(
    temp: &TempDir,
    lower: &Ids,
    parent: &Ids,
    gateway: &mut WorkspaceGateway,
    kind: FsDispatchKind,
    prepared: PreparedToolCall,
    mut router: ToolRouter,
    mutation: CompiledMutation,
) -> (peritus_tool_router::DispatchOutcome, Option<MutationOutcome>) {
    let caller = caller(&prepared, parent);
    let patch = mutation.into_patch();
    let lower_intent =
        authority_support::intent(lower, patch_authorization_payload_for_caller(&patch, &caller));
    let lower_receipts = authority_support::receipts(temp, lower, &lower_intent);
    let lower_request = authority_support::exact_request(&lower_intent, &lower_receipts, lower)
        .with_caller_binding(caller);
    let parent_intent = tool_action_intent(
        &prepared,
        parent.actor,
        ActorRole::Writer,
        parent.environment,
        parent.resource,
    );
    let parent_receipts = authority_support::receipts(temp, parent, &parent_intent);
    let parent_request = tool_request(parent, &parent_intent, &parent_receipts, &prepared);
    let mut dispatcher = FsDispatcher::mutation(kind, gateway, &lower_request).expect("dispatcher");
    let outcome =
        router.dispatch(prepared, &parent_request, &mut dispatcher).expect("router dispatch");
    let mutation = dispatcher.take_mutation_outcome();
    (outcome, mutation)
}

const fn tool_request<'a>(
    ids: &Ids,
    intent: &'a ActionIntentDto,
    receipts: &'a AuthorityReceipts,
    prepared: &PreparedToolCall,
) -> ToolAuthorizationRequest<'a> {
    ToolAuthorizationRequest::new(
        intent,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        Some(&receipts.lease),
        &receipts.epoch,
        ids.revision,
        ids.session,
        receipts.observed_at,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        prepared.prepared_digest(),
    )
}

fn descriptor(name: &str) -> ToolDescriptor {
    descriptor_catalog()
        .expect("descriptor catalog")
        .into_iter()
        .find(|descriptor| descriptor.name().as_str() == name)
        .expect("selected descriptor")
}

pub const fn workspace_version(ids: &Ids) -> peritus_tools_fs::WorkspaceVersion {
    peritus_tools_fs::WorkspaceVersion::new(
        ids.workspace,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
    )
}

pub fn assert_success(outcome: peritus_tool_router::DispatchOutcome) {
    let peritus_tool_router::DispatchOutcome::Completed(result) = outcome else {
        panic!("filesystem mutation did not complete synchronously");
    };
    assert_eq!(result.status(), peritus_tool_protocol::ResultStatus::Succeeded);
}

pub fn assert_failure(outcome: peritus_tool_router::DispatchOutcome) {
    let peritus_tool_router::DispatchOutcome::Completed(result) = outcome else {
        panic!("filesystem rejection did not complete synchronously");
    };
    assert_eq!(result.status(), peritus_tool_protocol::ResultStatus::Failed);
}
