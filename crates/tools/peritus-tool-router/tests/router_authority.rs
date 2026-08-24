//! Durable authority, no-dispatch, replay, and active-ownership integration tests.

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_tool_protocol::{
    BoundedJson, BoundedText, CancellationReason, IdempotencySemantics, ImplementationIdentity,
    ToolControl, ToolDescriptor, ToolResult, ToolTiming,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ExecutionUpdate, RouterErrorKind,
    ToolDispatcher, ToolExecution, ToolStart, tool_action_intent,
};
use peritus_types::Sha256Digest;

use support::{Ids, TestRoot, authority_request, call, complete_truncation, router};

struct CompletingDispatcher {
    identity: ImplementationIdentity,
    digest: peritus_tool_protocol::SchemaDigest,
    calls: usize,
}

impl CompletingDispatcher {
    fn new(descriptor: &ToolDescriptor) -> Self {
        Self {
            identity: descriptor.implementation_identity().clone(),
            digest: descriptor.descriptor_digest(),
            calls: 0,
        }
    }
}

impl ToolDispatcher for CompletingDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }
    fn descriptor_digest(&self) -> peritus_tool_protocol::SchemaDigest {
        self.digest
    }
    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        self.calls += 1;
        let timing = ToolTiming::new(invocation.observed_at(), invocation.observed_at()).unwrap();
        let result = ToolResult::success(
            invocation.prepared(),
            BoundedJson::null(),
            BoundedText::new("ok".to_owned()).unwrap(),
            BoundedText::new("ok".to_owned()).unwrap(),
            Vec::new(),
            timing,
            complete_truncation(),
            0,
        )
        .unwrap();
        Ok(ToolStart::Completed(result))
    }
}

#[test]
fn exact_authority_dispatches_once_and_exact_replay_returns_cached_result() {
    let root = TestRoot::new();
    let ids = Ids::new(10);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "exact")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let request = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    assert!(matches!(
        router.dispatch(prepared.clone(), &request, &mut dispatcher).unwrap(),
        DispatchOutcome::Completed(_),
    ));
    assert!(matches!(
        router.dispatch(prepared, &request, &mut dispatcher).unwrap(),
        DispatchOutcome::Replayed(_),
    ));
    assert_eq!(dispatcher.calls, 1);
}

#[test]
fn exact_replay_still_requires_exact_authority_without_redispatch() {
    let root = TestRoot::new();
    let ids = Ids::new(20);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "replay-authority")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let exact = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    router.dispatch(prepared.clone(), &exact, &mut dispatcher).unwrap();

    let stale = authority_request(&ids, &intent, &receipts, Sha256Digest::new([0xff; 32]));
    let error = router.dispatch(prepared, &stale, &mut dispatcher).unwrap_err();
    assert_eq!(error.kind(), RouterErrorKind::Authorization);
    assert_eq!(dispatcher.calls, 1);
}

#[test]
fn authority_rejection_never_calls_dispatcher() {
    let root = TestRoot::new();
    let ids = Ids::new(30);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "missing-dispatch")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, false);
    let request = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    let error = router.dispatch(prepared, &request, &mut dispatcher).unwrap_err();
    assert_eq!(error.kind(), RouterErrorKind::Authorization);
    assert_eq!(dispatcher.calls, 0);
}

#[test]
fn wrong_prepared_digest_never_calls_dispatcher() {
    let root = TestRoot::new();
    let ids = Ids::new(50);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "wrong-digest")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let request = authority_request(&ids, &intent, &receipts, Sha256Digest::new([99; 32]));
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    let error = router.dispatch(prepared, &request, &mut dispatcher).unwrap_err();
    assert_eq!(error.kind(), RouterErrorKind::Authorization);
    assert_eq!(dispatcher.calls, 0);
}

#[test]
fn conflicting_and_non_idempotent_replay_never_repeat_effect() {
    let root = TestRoot::new();
    let ids = Ids::new(70);
    let mut router = router(IdempotencySemantics::ReportPriorOutcome);
    let prepared = router.prepare(call(&ids, "first")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let request = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = CompletingDispatcher::new(prepared.descriptor());
    router.dispatch(prepared.clone(), &request, &mut dispatcher).unwrap();
    assert!(matches!(
        router.dispatch(prepared, &request, &mut dispatcher).unwrap(),
        DispatchOutcome::PriorOutcome(_),
    ));
    let conflict = router.prepare(call(&ids, "changed")).unwrap();
    let error = router.dispatch(conflict, &request, &mut dispatcher).unwrap_err();
    assert_eq!(error.kind(), RouterErrorKind::ReplayConflict);
    assert_eq!(dispatcher.calls, 1);
}

struct ActiveDispatcher {
    identity: ImplementationIdentity,
    digest: peritus_tool_protocol::SchemaDigest,
    dropped: Arc<AtomicUsize>,
}

impl ToolDispatcher for ActiveDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }
    fn descriptor_digest(&self) -> peritus_tool_protocol::SchemaDigest {
        self.digest
    }
    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        Ok(ToolStart::Active(Box::new(CompletingExecution {
            prepared: invocation.prepared().clone(),
            observed_at: invocation.observed_at(),
            dropped: Arc::clone(&self.dropped),
        })))
    }
}

struct CompletingExecution {
    prepared: peritus_tool_protocol::PreparedToolCall,
    observed_at: AuthorityInstant,
    dropped: Arc<AtomicUsize>,
}

impl Drop for CompletingExecution {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl ToolExecution for CompletingExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        let timing = ToolTiming::new(self.observed_at, observed_at).unwrap();
        let result = ToolResult::success(
            &self.prepared,
            BoundedJson::null(),
            BoundedText::new("ok".to_owned()).unwrap(),
            BoundedText::new("ok".to_owned()).unwrap(),
            Vec::new(),
            timing,
            complete_truncation(),
            0,
        )
        .unwrap();
        Ok(ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result)).unwrap())
    }
    fn control(
        &mut self,
        _control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(observed_at)
    }
    fn cancel(
        &mut self,
        _reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(observed_at)
    }
    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<peritus_tool_router::RecoveryObservation, DispatchFailure> {
        Ok(peritus_tool_router::RecoveryObservation::Active(self.poll(observed_at)?))
    }
}

#[test]
fn rejected_control_restores_active_execution_ownership() {
    let root = TestRoot::new();
    let ids = Ids::new(90);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "active")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 1_000, true);
    let request = authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut dispatcher = ActiveDispatcher {
        identity: prepared.descriptor().implementation_identity().clone(),
        digest: prepared.descriptor_digest(),
        dropped: Arc::clone(&dropped),
    };
    let DispatchOutcome::Active(handle) =
        router.dispatch(prepared, &request, &mut dispatcher).unwrap()
    else {
        panic!("expected active invocation");
    };
    let unsupported = ToolControl::stdin(vec![1], 8).unwrap();
    assert_eq!(
        router.control(handle, unsupported, support::instant(21)).unwrap_err().kind(),
        RouterErrorKind::Control,
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert!(router.poll(handle, support::instant(22)).unwrap().terminal().is_some());
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
