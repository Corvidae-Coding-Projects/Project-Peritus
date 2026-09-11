//! The stopped empty-folder failure sequence stays bounded and retains all observed work.
#[allow(dead_code, reason = "shared integration support serves sibling composition fixtures")]
#[path = "production_composition/support.rs"]
mod support;

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ModelFailure, ModelLimits, ModelRequest,
    OutcomeCertainty, ProviderName, ProviderProfile, RedactedDiagnostic, Retryability,
    TransportPhase, UsageCounters, UsageObservation, UsageScope,
};
use peritus_product_runner::{
    PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope, ProductRunInput, ProductRunner,
    ProductWorkspaceKind, RoleProviders,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

struct RecordingProvider {
    inner: support::ScriptedProvider,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ModelProvider for RecordingProvider {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.start(request, cancellation)
    }
}

#[test]
fn absent_reads_do_not_replenish_recovery_and_failed_roles_are_fully_accounted() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let base = support::profile([0xc1; 16], "in-place-recovery");
        let profile = ProviderProfile::new(base.profile_id(), 1, base.provider().clone(), base.model().clone(), base.dialect(), base.capabilities(), base.provenance(), ModelLimits::new(32_768, 4096, 64, 1, 512 * 1024).unwrap(), base.output_limit_enforcement(), base.state_mode(), base.resume_kind(), base.cancellation_kind()).unwrap();
        let mut responses = VecDeque::from([
            support::named_tool_response("workspace_list", support::list_arguments(".", 3)),
            support::design_response(),
        ]);
        for _ in 0..3 {
            responses.extend([
                support::named_tool_response("workspace_list", support::list_arguments(".", 3)),
                support::named_tool_response("workspace_read", support::read_arguments("Cargo.toml")),
                malformed_response(),
            ]);
        }
        let provider = Arc::new(RecordingProvider {
            inner: support::ScriptedProvider { profile, responses: Mutex::new(responses.into_iter().map(metered).collect()) },
            requests: Mutex::new(Vec::new()),
        });
        let writer: Arc<dyn ModelProvider> = provider.clone();
        let run_id = RunId::new([0xc2; 16]).unwrap();
        let updates = Arc::new(Mutex::new(Vec::new()));
        let observer_updates = Arc::clone(&updates);
        let outcome = ProductRunner::run(ProductRunInput {
            workspace_kind: ProductWorkspaceKind::InPlace { protected_paths: vec![], baseline_revision: 1 },
            run_id,
            workspace_id: WorkspaceId::new([0xc3; 16]).unwrap(),
            workspace_root: workspace.path().to_owned(),
            trace_path: state.path().join("run.trace"),
            command_runtime: support::command_runtime(state.path(), workspace.path(), run_id),
            finding_state: String::new(),
            task: "Create a Rust game in this empty folder.".to_owned(),
            max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
            delivery_scope: ProductDeliveryScope::WorkspaceChanges,
            conversation: Arc::new(support::FixedConversation("Create a Rust game in this empty folder.".to_owned())),
            providers: RoleProviders { writer: writer.clone(), reviewer: writer.clone(), fixer: writer, fallbacks: vec![] },
            cancelled: Arc::new(AtomicBool::new(false)),
            provider_cancellation: CancellationToken::new(),
            resume: None,
        }, Arc::new(move |update| observer_updates.lock().unwrap().push(update))).await.unwrap();
        assert!(!outcome.settlement().is_accepted());
        assert!(outcome.candidate().is_none());
        assert!(std::fs::read_dir(workspace.path()).unwrap().next().is_none());
        let requests = provider.requests.lock().unwrap();
        assert_eq!(requests.len(), 11, "no uncounted fourth writer invocation");
        let corrected = requests[5].messages().iter().flat_map(peritus_model_protocol::Message::content).any(|block| {
            matches!(block, peritus_model_protocol::ContentBlock::Text(text) if text.expose_for_wire().contains("recoverable `malformed_payload`") && text.expose_for_wire().contains("encode arguments_json exactly once"))
        });
        drop(requests);
        assert!(corrected, "first failure must receive the recovery correction");
        let updates = updates.lock().unwrap();
        let progress = updates.last().unwrap().progress;
        drop(updates);
        assert_eq!(progress.model_requests(), 11);
        assert_eq!(progress.tool_calls(), 7);
        assert_eq!(progress.retries(), 2);
        assert_eq!(progress.input_tokens(), 110);
        assert_eq!(progress.output_tokens(), 22);
        assert_eq!(progress.total_tokens(), 132);
        assert_eq!(progress.usage_observations(), 11);
    });
}

fn malformed_response() -> VecDeque<EventEnvelope> {
    failure_response(FailureCategory::MalformedPayload, "openai.codex_runtime.invalid_envelope")
}

#[test]
fn material_progress_cannot_restart_a_safety_terminal_or_switch_providers() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let workspace = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let base = support::profile([0xd1; 16], "progress-safety");
        let profile = ProviderProfile::new(
            base.profile_id(),
            1,
            base.provider().clone(),
            base.model().clone(),
            base.dialect(),
            base.capabilities(),
            base.provenance(),
            ModelLimits::new(32_768, 4096, 64, 1, 512 * 1024).unwrap(),
            base.output_limit_enforcement(),
            base.state_mode(),
            base.resume_kind(),
            base.cancellation_kind(),
        )
        .unwrap();
        let provider = Arc::new(RecordingProvider {
            inner: support::ScriptedProvider {
                profile,
                responses: Mutex::new(VecDeque::from([
                    support::named_tool_response("workspace_list", support::list_arguments(".", 3)),
                    support::design_response(),
                    support::named_tool_response("workspace_list", support::list_arguments(".", 3)),
                    support::tool_response(support::write_arguments(
                        "candidate.txt",
                        "retained work",
                    )),
                    failure_response(FailureCategory::Safety, "openai.codex_runtime.native_tool"),
                ])),
            },
            requests: Mutex::new(Vec::new()),
        });
        let fallback = Arc::new(RecordingProvider {
            inner: support::ScriptedProvider {
                profile: support::profile([0xd4; 16], "unused-fallback"),
                responses: Mutex::new(VecDeque::new()),
            },
            requests: Mutex::new(Vec::new()),
        });
        let writer: Arc<dyn ModelProvider> = provider.clone();
        let run_id = RunId::new([0xd2; 16]).unwrap();
        let updates = Arc::new(Mutex::new(Vec::new()));
        let observer_updates = Arc::clone(&updates);
        let outcome = ProductRunner::run(
            ProductRunInput {
                workspace_kind: ProductWorkspaceKind::InPlace {
                    protected_paths: vec![],
                    baseline_revision: 1,
                },
                run_id,
                workspace_id: WorkspaceId::new([0xd3; 16]).unwrap(),
                workspace_root: workspace.path().to_owned(),
                trace_path: state.path().join("run.trace"),
                command_runtime: support::command_runtime(state.path(), workspace.path(), run_id),
                finding_state: String::new(),
                task: "Create a text file in this empty folder.".to_owned(),
                max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                conversation: Arc::new(support::FixedConversation(
                    "Create a text file in this empty folder.".to_owned(),
                )),
                providers: RoleProviders {
                    writer: writer.clone(),
                    reviewer: writer.clone(),
                    fixer: writer,
                    fallbacks: vec![fallback.clone()],
                },
                cancelled: Arc::new(AtomicBool::new(false)),
                provider_cancellation: CancellationToken::new(),
                resume: None,
            },
            Arc::new(move |update| observer_updates.lock().unwrap().push(update)),
        )
        .await
        .unwrap();
        assert!(!outcome.settlement().is_accepted());
        assert!(outcome.candidate().is_some(), "prior effects remain an unaccepted candidate");
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("candidate.txt")).unwrap(),
            "retained work"
        );
        assert_eq!(provider.requests.lock().unwrap().len(), 5, "no same-provider restart");
        assert!(fallback.requests.lock().unwrap().is_empty(), "no fallback on a safety terminal");
        let progress = updates.lock().unwrap().last().unwrap().progress;
        assert_eq!(progress.retries(), 0);
        assert_eq!(progress.provider_failovers(), 0);
    });
}

fn failure_response(category: FailureCategory, code: &str) -> VecDeque<EventEnvelope> {
    let failure = ModelFailure::new(
        ProviderName::new("openai".to_owned()).unwrap(),
        category,
        TransportPhase::ReadingBody,
        OutcomeCertainty::MaybeAccepted,
        Retryability::Never,
        None,
        None,
        None,
        RedactedDiagnostic::new(code.to_owned(), None, None, None).unwrap(),
    );
    envelopes(vec![
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ResponseFailed(failure),
    ])
}

fn metered(events: VecDeque<EventEnvelope>) -> VecDeque<EventEnvelope> {
    let mut events: Vec<_> = events.into_iter().map(|event| event.event().clone()).collect();
    let index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                ModelEvent::Finish(_)
                    | ModelEvent::ResponseFailed(_)
                    | ModelEvent::ResponseCompleted
            )
        })
        .unwrap();
    events.insert(
        index,
        ModelEvent::Usage(UsageObservation::new(
            UsageScope::Final,
            UsageCounters::new(Some(10), None, None, Some(2), None, None, Some(12), None),
            None,
        )),
    );
    envelopes(events)
}

fn envelopes(events: Vec<ModelEvent>) -> VecDeque<EventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            EventEnvelope::new(index as u64 + 1, None, None, Sha256Digest::new([1; 32]), event)
                .unwrap()
        })
        .collect()
}
