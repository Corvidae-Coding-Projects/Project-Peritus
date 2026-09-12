//! A retried provider proposal cannot relaunch an effect with an unfinalized durable receipt.

use super::*;
use crate::developer_tools::receipt::{EffectReceiptLedger, ReceiptDecision};
use peritus_agent::{
    DeveloperLoop, DeveloperLoopError, DeveloperLoopLimits, DeveloperLoopRequest,
    DeveloperRetryRecord, DeveloperTrace, DeveloperTraceEvent,
};
use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, EventEnvelope,
    FinishReason, ItemId, ItemKind, ModelEvent, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode, StreamFragment,
    WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Evidence {
    requests: Vec<ModelRequest>,
    observations: Vec<DeveloperToolObservation>,
    retries: Vec<DeveloperRetryRecord>,
    disconnects: usize,
    failed_token: Option<CancellationToken>,
}

struct Trace(Arc<Mutex<Evidence>>);

impl DeveloperTrace for Trace {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        let mut evidence = self.0.lock().expect("evidence");
        match event {
            DeveloperTraceEvent::ToolObservation { observation, .. } => {
                evidence.observations.push(DeveloperToolObservation {
                    output: observation.output.clone(),
                    is_error: observation.is_error,
                });
            }
            DeveloperTraceEvent::RetryScheduled(record) => evidence.retries.push(*record),
            DeveloperTraceEvent::ProviderEnvelope(_)
            | DeveloperTraceEvent::ContextCompaction(_) => {}
        }
        drop(evidence);
        Ok(())
    }
}

struct Provider {
    profile: ProviderProfile,
    call: CompletedToolCall,
    disconnect: bool,
    counter: PathBuf,
    prior_effect: bool,
    evidence: Arc<Mutex<Evidence>>,
}

impl ModelProvider for Provider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        let mut evidence = self.evidence.lock().expect("evidence");
        let disconnect = self.disconnect && evidence.requests.is_empty();
        evidence.requests.push(request);
        drop(evidence);
        let mut events = tool_response(&self.call);
        if disconnect {
            events.truncate(5); // Retain ItemCompleted; omit Finish and ResponseCompleted.
        }
        let stream = Stream {
            events,
            disconnect,
            completed_emitted: false,
            counter: self.counter.clone(),
            prior_effect: self.prior_effect,
            evidence: Arc::clone(&self.evidence),
        };
        Box::pin(async move { Ok(OwnedModelStream::new(stream, cancellation)) })
    }
}

struct Stream {
    events: VecDeque<EventEnvelope>,
    disconnect: bool,
    completed_emitted: bool,
    counter: PathBuf,
    prior_effect: bool,
    evidence: Arc<Mutex<Evidence>>,
}

impl ModelStream for Stream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if let Some(event) = self.events.pop_front() {
                self.completed_emitted |= matches!(event.event(), ModelEvent::ItemCompleted(_));
                return Ok(Some(event));
            }
            if self.disconnect {
                // This pull occurs only after the loop accepted the preceding ItemCompleted.
                // Record and check the reached boundary before injecting the transport failure.
                assert!(self.completed_emitted);
                assert_counter(&self.counter, self.prior_effect);
                let mut evidence = self.evidence.lock().expect("evidence");
                assert!(evidence.observations.is_empty(), "unaccepted response executed a tool");
                evidence.disconnects += 1;
                evidence.failed_token = Some(cancellation.clone());
                drop(evidence);
                return Err(ProviderCoreError::transport(
                    "pairwise_fixture",
                    "disconnected after tool completion",
                ));
            }
            Ok(None)
        })
    }
}

#[test]
fn disconnect_retry_cannot_relaunch_a_durably_pending_command() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(
        async {
            for (pending, disconnect) in [(true, true), (false, true), (true, false)] {
                tokio::time::timeout(Duration::from_secs(10), check_case(pending, disconnect))
                    .await
                    .expect("bounded pairwise fixture");
            }
        },
    );
}

async fn check_case(pending: bool, disconnect: bool) {
    let workspace = tempfile::tempdir().expect("workspace");
    let receipts = workspace.path().join(".git/peritus-pair-effects.bin");
    let counter = workspace.path().join("effect-count");
    let scope = "writer-disconnect-pair";
    let call = effect_call();
    if pending {
        seed_pending_effect(workspace.path(), &receipts, scope, &call);
    }
    let evidence = Arc::new(Mutex::new(Evidence::default()));
    let provider = Provider {
        profile: profile(),
        call: call.clone(),
        disconnect,
        counter: counter.clone(),
        prior_effect: pending,
        evidence: Arc::clone(&evidence),
    };
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        WorkspaceOwnership::capture(workspace.path()),
        receipts.clone(),
        scope.to_owned(),
        Duration::from_secs(5),
        super::super::test_command_runtime(workspace.path()),
    );
    // Ground through the real read-only owner interfaces without consuming an effect ordinal.
    fs::write(workspace.path().join("README.md"), "fixture\n").expect("grounding file");
    for call in [
        completed_call("list", "workspace_list", r#"{"depth":1,"path":""}"#),
        completed_call("read", "workspace_read", r#"{"path":"README.md"}"#),
    ] {
        assert!(!tools.execute(&call).expect("grounding").is_error);
    }
    let caller = CancellationToken::new();
    let result = DeveloperLoop::run(
        &provider,
        DeveloperLoopRequest {
            request_prefix: scope.to_owned(),
            system: "Run the authorized fixture once.".to_owned(),
            prompt: "Continue the interrupted command.".to_owned(),
            attachments: Vec::new(),
            tools: crate::developer_tools::definitions().expect("tools"),
            limits: DeveloperLoopLimits::new(1, 1)
                .and_then(|limits| limits.with_max_attempts_per_turn(2))
                .expect("limits"),
            cancellation: caller.clone(),
        },
        &mut tools,
        &mut Trace(Arc::clone(&evidence)),
    )
    .await;
    // A single admitted tool turn intentionally exhausts the fixture's turn budget.
    assert!(matches!(result, Err(DeveloperLoopError::LimitExceeded)), "{:?}", result.err());
    assert!(!caller.is_cancelled());
    assert_counter(&counter, true);
    check_evidence(&evidence.lock().expect("evidence"), pending, disconnect);
    if pending {
        let mut reopened = EffectReceiptLedger::new(receipts, scope.to_owned());
        assert!(matches!(
            reopened.begin(&call).expect("durable ambiguity"),
            ReceiptDecision::Refuse { ambiguous: true, .. }
        ));
    }
}

fn check_evidence(evidence: &Evidence, pending: bool, disconnect: bool) {
    assert_eq!(evidence.requests.len(), 1 + usize::from(disconnect));
    assert_eq!(evidence.disconnects, usize::from(disconnect));
    assert_eq!(evidence.retries.len(), usize::from(disconnect));
    if disconnect {
        assert_eq!(evidence.requests[0].messages(), evidence.requests[1].messages());
        assert!(evidence.failed_token.as_ref().expect("failed attempt").is_cancelled());
    }
    let [observation] = evidence.observations.as_slice() else { panic!("one tool observation") };
    assert_eq!(observation.is_error, pending, "{}", wire(observation));
    if pending {
        assert!(wire(observation).contains("ambiguous prior command outcome"));
    }
}

fn effect_call() -> CompletedToolCall {
    completed_call("pending-command", "run_command", &serde_json::json!({
        "program": std::env::current_exe().expect("test executable"),
        "args": ["--exact", "developer_tools::executor::receipt_tests::append_effect_counter_fixture", "--ignored"],
        "cwd": ".", "purpose": "external_effect",
    }).to_string())
}

fn seed_pending_effect(root: &Path, receipts: &Path, scope: &str, call: &CompletedToolCall) {
    let mut ledger = EffectReceiptLedger::new(receipts.to_owned(), scope.to_owned());
    assert!(matches!(ledger.begin(call).expect("durable start"), ReceiptDecision::Execute));
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "developer_tools::executor::receipt_tests::append_effect_counter_fixture",
            "--ignored",
        ])
        .current_dir(root)
        .status()
        .expect("initial effect subprocess");
    assert!(status.success());
    assert_counter(&root.join("effect-count"), true);
    // Simulated interruption after actual dispatch, before receipt finalization. No completed
    // record is fabricated, so the recovery path must conservatively refuse redispatch.
}

fn assert_counter(path: &Path, exists: bool) {
    if exists {
        assert_eq!(fs::read_to_string(path).expect("effect counter"), "effect\n");
    } else {
        assert!(!path.exists(), "effect dispatched before response acceptance");
    }
}

fn tool_response(call: &CompletedToolCall) -> VecDeque<EventEnvelope> {
    let item = ItemId::new("command-item".to_owned()).expect("item");
    [
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.id().clone(),
            name: call.name().clone(),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call.id().clone(),
            fragment: StreamFragment::new(
                call.arguments().canonical_bytes().to_vec(),
                ProtocolLimits::PRODUCTION,
            )
            .expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, event)| {
        EventEnvelope::new(
            u64::try_from(index + 1).expect("sequence"),
            None,
            None,
            Sha256Digest::new([1; 32]),
            event,
        )
        .expect("envelope")
    })
    .collect()
}

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x75; 16]).expect("profile ID"),
        1,
        ProviderName::new("pairwise-provider".to_owned()).expect("provider"),
        ModelName::new("pairwise-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4096, 64, 1, 256 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}
