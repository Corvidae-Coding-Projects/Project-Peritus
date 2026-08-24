//! Exact router and authority assembly for Git production-flow tests.

use std::sync::Arc;

use peritus_artifact_store::ArtifactStore;
use peritus_git::CandidateSnapshot;
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
use peritus_tools_git::{GitDispatcher, GitMutationOutcome, descriptor_catalog};
use peritus_types::SnapshotId;
use peritus_workspace::{
    MutationOutcome, RollbackRequest, WorkspaceCallerBinding, WorkspaceGateway,
    candidate_authorization_payload_for_caller, rollback_authorization_payload_for_caller,
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

#[allow(clippy::too_many_arguments, reason = "the fixture binds two authorities and one effect")]
pub fn dispatch_candidate(
    temp: &TempDir,
    lower: &Ids,
    parent: &Ids,
    gateway: &mut WorkspaceGateway,
    mutation: &MutationOutcome,
    snapshot: SnapshotId,
    artifacts: &ArtifactStore,
    prepared: PreparedToolCall,
    mut router: ToolRouter,
) -> (peritus_tool_router::DispatchOutcome, Option<GitMutationOutcome>) {
    let caller = caller(&prepared, parent);
    let lower_intent = authority_support::intent(
        lower,
        candidate_authorization_payload_for_caller(mutation, snapshot, &caller),
    );
    let lower_receipts = authority_support::receipts(temp, lower, &lower_intent);
    let lower_request = authority_support::exact_request(&lower_intent, &lower_receipts, lower)
        .with_caller_binding(caller);
    let parent_intent = parent_intent(&prepared, parent);
    let parent_receipts = authority_support::receipts(temp, parent, &parent_intent);
    let parent_request = tool_request(parent, &parent_intent, &parent_receipts, &prepared);
    let mut dispatcher =
        GitDispatcher::candidate(gateway, &lower_request, mutation, artifacts).expect("dispatcher");
    let outcome = router
        .dispatch(prepared, &parent_request, &mut dispatcher)
        .expect("router candidate dispatch");
    let mutation = dispatcher.take_mutation_outcome();
    (outcome, mutation)
}

#[allow(clippy::too_many_arguments)]
pub fn dispatch_rollback(
    temp: &TempDir,
    lower: &Ids,
    parent: &Ids,
    gateway: &mut WorkspaceGateway,
    target: &CandidateSnapshot,
    successor: SnapshotId,
    artifacts: &ArtifactStore,
    prepared: PreparedToolCall,
    mut router: ToolRouter,
) -> (peritus_tool_router::DispatchOutcome, Option<GitMutationOutcome>) {
    let caller = caller(&prepared, parent);
    let request = RollbackRequest::new(target, successor);
    let lower_intent = authority_support::intent(
        lower,
        rollback_authorization_payload_for_caller(gateway.state(), &request, &caller),
    );
    let lower_receipts = authority_support::receipts(temp, lower, &lower_intent);
    let lower_request = authority_support::exact_request(&lower_intent, &lower_receipts, lower)
        .with_caller_binding(caller);
    let parent_intent = parent_intent(&prepared, parent);
    let parent_receipts = authority_support::receipts(temp, parent, &parent_intent);
    let parent_request = tool_request(parent, &parent_intent, &parent_receipts, &prepared);
    let mut dispatcher =
        GitDispatcher::rollback(gateway, &lower_request, target, artifacts).expect("dispatcher");
    let outcome = router
        .dispatch(prepared, &parent_request, &mut dispatcher)
        .expect("router rollback dispatch");
    let mutation = dispatcher.take_mutation_outcome();
    (outcome, mutation)
}

fn caller(prepared: &PreparedToolCall, ids: &Ids) -> WorkspaceCallerBinding {
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

fn parent_intent(prepared: &PreparedToolCall, ids: &Ids) -> ActionIntentDto {
    tool_action_intent(prepared, ids.actor, ActorRole::Writer, ids.environment, ids.resource)
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

pub fn assert_success(outcome: peritus_tool_router::DispatchOutcome) {
    let peritus_tool_router::DispatchOutcome::Completed(result) = outcome else {
        panic!("Git mutation did not complete synchronously");
    };
    assert_eq!(result.status(), peritus_tool_protocol::ResultStatus::Succeeded);
}

pub fn snapshot_hex(value: SnapshotId) -> String {
    let mut output = String::with_capacity(32);
    for byte in value.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("hex rendering");
    }
    output
}
