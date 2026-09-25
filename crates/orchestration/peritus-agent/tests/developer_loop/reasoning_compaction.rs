//! Long reasoning-bearing sessions still compact, and only display summaries reach observers.
use super::*;
use peritus_agent::{
    DeveloperActivity, DeveloperInput, DeveloperInteraction, DeveloperLoopError,
    DeveloperRequestAdmission,
};
use peritus_model_protocol::{Capability, CompletedToolCall, Message};

#[derive(Default)]
struct Observer(Mutex<Vec<String>>);

impl DeveloperInteraction for Observer {
    fn allows_semantic_compaction(&self) -> bool {
        false
    }
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        Ok(DeveloperInput {
            revision: 1,
            conversation: "Inspect the workspace".to_owned(),
            images: Vec::new(),
        })
    }
    fn prepare_request(
        &self,
        _: u64,
        _: &ModelRequest,
    ) -> Result<DeveloperRequestAdmission, DeveloperLoopError> {
        Ok(DeveloperRequestAdmission::Accepted)
    }
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        match activity {
            DeveloperActivity::Text(bytes) | DeveloperActivity::ReasoningSummary(bytes) => {
                self.0.lock().unwrap().push(String::from_utf8(bytes.to_vec()).unwrap());
            }
            _ => {}
        }
        Ok(())
    }
}

struct VerboseTool;
impl DeveloperToolExecutor for VerboseTool {
    fn execute(
        &mut self,
        _: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(
                &format!(r#"{{"content":"{}"}}"#, "workspace evidence ".repeat(1100)),
                JsonBounds::value(ProtocolLimits::PRODUCTION),
            )?,
            is_error: false,
        })
    }
}

#[test]
fn reasoning_tool_history_compacts_and_replay_never_becomes_public_text() {
    for opaque in [true, false] {
        block_on(async {
            let replay =
                br#"{"service":"deepseek","fields":{"reasoning_content":"PRIVATE_REPLAY_CANARY"}}"#;
            let response = reasoning_tests::reasoning_response(opaque, replay);
            let mut responses = VecDeque::from(vec![response; 24]);
            responses.push_back(text_response());
            let provider = ScriptedProvider {
                profile: fixtures::profile_with_capabilities(if opaque {
                    &[Capability::ToolCalls, Capability::ReasoningReplay]
                } else {
                    &[
                        Capability::ToolCalls,
                        Capability::ReasoningReplay,
                        Capability::ReasoningControls,
                        Capability::ReasoningSummaries,
                    ]
                }),
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            };
            let observer = Observer::default();
            let outcome = DeveloperLoop::run_interactive(
                &provider,
                DeveloperLoopRequest {
                    request_prefix: "reasoning-compaction".to_owned(),
                    system: "Inspect then finish".to_owned(),
                    prompt: "Read the workspace".to_owned(),
                    attachments: Vec::new(),
                    tools: vec![read_tool()],
                    limits: DeveloperLoopLimits::new(26, 26).unwrap(),
                    cancellation: CancellationToken::new(),
                },
                &mut VerboseTool,
                &mut RecordingTrace::default(),
                None,
                &observer,
            )
            .await
            .expect("reasoning must not block compaction");
            assert!(outcome.compactions > 0);
            let requests = provider.requests.lock().unwrap();
            assert_eq!(requests.len(), 25);
            assert_eq!(
                matches!(
                    requests[0].options().reasoning(),
                    ReasoningPolicy::Effort { summary: SummaryPolicy::Auto, .. }
                ),
                !opaque
            );
            assert!(requests.iter().all(
                |request| peritus_agent::estimate_developer_request_tokens(
                    request.messages(),
                    request.tools()
                ) <= 32_768
            ));
            assert!(requests.last().unwrap().messages().iter().flat_map(Message::content).any(|block| matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("source_sha256="))));
            assert!(!requests.iter().flat_map(ModelRequest::messages).flat_map(Message::content).any(|block| matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("PRIVATE_REPLAY_CANARY"))));
            drop(requests);
            let public = observer.0.lock().unwrap().join("");
            assert!(!public.contains("PRIVATE_REPLAY_CANARY"));
            assert_eq!(public.contains("visible summary"), !opaque);
        });
    }
}
