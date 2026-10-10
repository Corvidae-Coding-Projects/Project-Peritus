//! Explicit untimed calls retain authority fences and cancellation without elapsed expiry.

mod support;

use std::sync::{Arc, Mutex};

use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_tool_protocol::{
    CancellationReason, IdempotencySemantics, ImplementationIdentity, PreparedToolCall,
    SchemaDigest, ToolControl,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ExecutionUpdate, RecoveryObservation,
    ToolDispatcher, ToolExecution, ToolStart, tool_action_intent,
};

#[derive(Default)]
struct Calls {
    polls: usize,
    cancellations: Vec<CancellationReason>,
}

struct Dispatcher {
    identity: ImplementationIdentity,
    digest: SchemaDigest,
    calls: Arc<Mutex<Calls>>,
}

impl ToolDispatcher for Dispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }
    fn descriptor_digest(&self) -> SchemaDigest {
        self.digest
    }
    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        Ok(ToolStart::Active(Box::new(Execution {
            prepared: invocation.into_prepared(),
            calls: self.calls.clone(),
        })))
    }
}

struct Execution {
    prepared: PreparedToolCall,
    calls: Arc<Mutex<Calls>>,
}

impl Execution {
    fn pending(&self) -> ExecutionUpdate {
        ExecutionUpdate::new(&self.prepared, Vec::new(), None).unwrap()
    }
}

impl ToolExecution for Execution {
    fn poll(&mut self, _: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.calls.lock().unwrap().polls += 1;
        Ok(self.pending())
    }
    fn control(
        &mut self,
        _: ToolControl,
        at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(at)
    }
    fn cancel(
        &mut self,
        reason: CancellationReason,
        _: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.calls.lock().unwrap().cancellations.push(reason);
        Ok(self.pending())
    }
    fn recover(&mut self, _: AuthorityInstant) -> Result<RecoveryObservation, DispatchFailure> {
        Ok(RecoveryObservation::Active(self.pending()))
    }
}

#[test]
fn long_running_untimed_call_still_cancels_explicitly_and_on_epoch_change() {
    let root = support::TestRoot::new();
    let ids = support::Ids::new(190);
    let mut router = support::router_without_timeout(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(support::call_without_timeout(&ids, "untimed")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(&root);
    let receipts = support::commit_authority(&mut journal, &ids, &intent, 0, true);
    let request = support::authority_request(&ids, &intent, &receipts, prepared.prepared_digest());
    let calls = Arc::new(Mutex::new(Calls::default()));
    let mut dispatcher = Dispatcher {
        identity: prepared.descriptor().implementation_identity().clone(),
        digest: prepared.descriptor_digest(),
        calls: calls.clone(),
    };
    let DispatchOutcome::Active(handle) =
        router.dispatch(prepared, &request, &mut dispatcher).unwrap()
    else {
        panic!("active invocation");
    };
    router.poll(handle, support::instant(1_000_000)).unwrap();
    assert_eq!(calls.lock().unwrap().polls, 1);
    assert!(calls.lock().unwrap().cancellations.is_empty());
    router
        .control(
            handle,
            ToolControl::Cancel(CancellationReason::Requested),
            support::instant(1_000_001),
        )
        .unwrap();
    assert_eq!(calls.lock().unwrap().cancellations, [CancellationReason::Requested]);
    router
        .poll(handle, AuthorityInstant::new(peritus_types::Generation::new(2).unwrap(), 1_000_002))
        .unwrap();
    assert_eq!(
        calls.lock().unwrap().cancellations,
        [CancellationReason::Requested, CancellationReason::Deadline]
    );
}
