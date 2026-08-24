//! Progress-count, failure-normalization, and recovery lifecycle regressions.

mod support;

use peritus_policy::{ActorRole, AuthorityInstant};
use peritus_tool_protocol::{
    BoundedJson, BoundedText, IdempotencySemantics, ImplementationIdentity, ProgressKind,
    ResultStatus, ToolControl, ToolFailure, ToolProgress, ToolResult, ToolTiming,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ExecutionUpdate, InvocationHandle,
    RecoveryObservation, RecoveryOutcome, ReplayDisposition, RouterErrorKind, ToolDispatcher,
    ToolExecution, ToolRouter, ToolStart, tool_action_intent,
};

use support::{Ids, TestRoot, authority_request, call, complete_truncation, router};

#[derive(Clone, Copy)]
enum Script {
    ProgressThenFailure,
    RecoveryWithTerminal,
}

struct ScriptedDispatcher {
    identity: ImplementationIdentity,
    digest: peritus_tool_protocol::SchemaDigest,
    script: Script,
}

impl ToolDispatcher for ScriptedDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> peritus_tool_protocol::SchemaDigest {
        self.digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        Ok(ToolStart::Active(Box::new(ScriptedExecution {
            prepared: invocation.into_prepared(),
            polls: 0,
            script: self.script,
        })))
    }
}

struct ScriptedExecution {
    prepared: peritus_tool_protocol::PreparedToolCall,
    polls: u32,
    script: Script,
}

impl ScriptedExecution {
    fn progress(&self, sequence: u32, observed_at: AuthorityInstant) -> ToolProgress {
        ToolProgress::new(
            &self.prepared,
            sequence,
            ProgressKind::Update,
            observed_at,
            None,
            BoundedText::new("progress".to_owned()).unwrap(),
        )
        .unwrap()
    }

    fn terminal(&self, observed_at: AuthorityInstant, progress_count: u32) -> ToolResult {
        ToolResult::success(
            &self.prepared,
            BoundedJson::null(),
            BoundedText::new("ok".to_owned()).unwrap(),
            BoundedText::new("ok".to_owned()).unwrap(),
            Vec::new(),
            ToolTiming::new(support::instant(20), observed_at).unwrap(),
            complete_truncation(),
            progress_count,
        )
        .unwrap()
    }
}

impl ToolExecution for ScriptedExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        if self.polls == 0 {
            self.polls = 1;
            return Ok(ExecutionUpdate::new(
                &self.prepared,
                vec![self.progress(0, observed_at)],
                None,
            )
            .unwrap());
        }
        match self.script {
            Script::ProgressThenFailure => Err(failure()),
            Script::RecoveryWithTerminal => Ok(ExecutionUpdate::new(
                &self.prepared,
                Vec::new(),
                Some(self.terminal(observed_at, 1)),
            )
            .unwrap()),
        }
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
        _reason: peritus_tool_protocol::CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll(observed_at)
    }

    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        match self.script {
            Script::ProgressThenFailure => Err(failure()),
            Script::RecoveryWithTerminal => {
                let progress = self.progress(1, observed_at);
                let result = self.terminal(observed_at, 2);
                let update =
                    ExecutionUpdate::new(&self.prepared, vec![progress], Some(result)).unwrap();
                Ok(RecoveryObservation::Completed(update))
            }
        }
    }
}

#[test]
fn progress_then_typed_poll_failure_closes_with_observed_count() {
    let root = TestRoot::new();
    let ids = Ids::new(110);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let handle = start_active(&mut router, &root, &ids, Script::ProgressThenFailure);

    let first = router.poll(handle, support::instant(21)).unwrap();
    assert_eq!(first.progress()[0].sequence(), 0);
    let terminal = router.poll(handle, support::instant(22)).unwrap();
    let result = terminal.terminal().expect("normalized terminal failure");
    assert_eq!(result.status(), ResultStatus::Failed);
    assert_eq!(result.progress_count(), 1);
}

#[test]
fn recovery_progress_and_terminal_are_accepted_as_one_update() {
    let root = TestRoot::new();
    let ids = Ids::new(130);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let handle = start_active(&mut router, &root, &ids, Script::RecoveryWithTerminal);

    router.poll(handle, support::instant(21)).unwrap();
    let RecoveryOutcome::Completed(result) = router.recover(handle, support::instant(22)).unwrap()
    else {
        panic!("expected recovered terminal result");
    };
    assert_eq!(result.status(), ResultStatus::Succeeded);
    assert_eq!(result.progress_count(), 2);
}

#[test]
fn synchronous_result_cannot_claim_unobserved_progress() {
    let root = TestRoot::new();
    let ids = Ids::new(150);
    let mut router = router(IdempotencySemantics::ReplayTerminal);
    let prepared = router.prepare(call(&ids, "sync-progress")).unwrap();
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
    let mut dispatcher = DishonestSynchronousDispatcher {
        identity: prepared.descriptor().implementation_identity().clone(),
        digest: prepared.descriptor_digest(),
    };

    let error = router.dispatch(prepared.clone(), &request, &mut dispatcher).unwrap_err();
    assert_eq!(error.kind(), RouterErrorKind::InvalidObservation);
    assert!(matches!(
        router.dispatch(prepared, &request, &mut dispatcher).unwrap(),
        DispatchOutcome::PriorOutcome(ReplayDisposition::Indeterminate),
    ));
}

struct DishonestSynchronousDispatcher {
    identity: ImplementationIdentity,
    digest: peritus_tool_protocol::SchemaDigest,
}

impl ToolDispatcher for DishonestSynchronousDispatcher {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        &self.identity
    }

    fn descriptor_digest(&self) -> peritus_tool_protocol::SchemaDigest {
        self.digest
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        let result = ToolResult::success(
            invocation.prepared(),
            BoundedJson::null(),
            BoundedText::new("ok".to_owned()).unwrap(),
            BoundedText::new("ok".to_owned()).unwrap(),
            Vec::new(),
            ToolTiming::new(invocation.observed_at(), invocation.observed_at()).unwrap(),
            complete_truncation(),
            1,
        )
        .unwrap();
        Ok(ToolStart::Completed(result))
    }
}

fn start_active(
    router: &mut ToolRouter,
    root: &TestRoot,
    ids: &Ids,
    script: Script,
) -> InvocationHandle {
    let prepared = router.prepare(call(ids, "active-lifecycle")).unwrap();
    let intent = tool_action_intent(
        &prepared,
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        ids.resource,
    );
    let mut journal = support::open_journal(root);
    let receipts = support::commit_authority(&mut journal, ids, &intent, 1_000, true);
    let request = authority_request(ids, &intent, &receipts, prepared.prepared_digest());
    let mut dispatcher = ScriptedDispatcher {
        identity: prepared.descriptor().implementation_identity().clone(),
        digest: prepared.descriptor_digest(),
        script,
    };
    let DispatchOutcome::Active(handle) =
        router.dispatch(prepared, &request, &mut dispatcher).unwrap()
    else {
        panic!("expected active invocation");
    };
    handle
}

fn failure() -> DispatchFailure {
    DispatchFailure::new(
        ResultStatus::Failed,
        ToolFailure::new(
            peritus_tool_protocol::FailureCategory::Execution,
            BoundedText::new("execution_failed".to_owned()).unwrap(),
            peritus_tool_protocol::ResponsibleSubsystem::Tool,
            peritus_tool_protocol::Retryability::Never,
            peritus_tool_protocol::RecoveryRoute::None,
            BoundedText::new("λ failed".to_owned()).unwrap(),
        ),
    )
    .unwrap()
}
