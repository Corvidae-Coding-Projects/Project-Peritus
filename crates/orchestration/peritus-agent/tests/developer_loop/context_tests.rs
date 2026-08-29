//! Context accounting and caching behavior of the production developer loop.

use super::fixtures::{caching_profile, profile, read_tool, text_response, tool_response};
use super::*;
use peritus_model_protocol::CachePolicy;

#[derive(Default)]
struct VerboseTool {
    calls: u32,
}

impl DeveloperToolExecutor for VerboseTool {
    fn execute(
        &mut self,
        _call: &peritus_model_protocol::CompletedToolCall,
    ) -> Result<DeveloperToolObservation, peritus_agent::DeveloperLoopError> {
        self.calls += 1;
        let value = format!(r#"{{"content":"{}"}}"#, "x".repeat(20_000));
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(&value, JsonBounds::value(ProtocolLimits::PRODUCTION))?,
            is_error: false,
        })
    }
}

#[test]
fn developer_loop_negotiates_automatic_prompt_caching() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: caching_profile(),
            responses: Mutex::new(VecDeque::from([text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "cache-test".to_owned(),
                system: "Complete the task.".to_owned(),
                prompt: "Return the result.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(2, 2).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop");

        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].options().cache(), &CachePolicy::Automatic);
        drop(requests);
    });
}

#[test]
fn developer_loop_compacts_old_complete_tool_exchanges_with_digest_evidence() {
    block_on(async {
        let mut responses = VecDeque::new();
        for _ in 0..6 {
            responses.push_back(tool_response());
        }
        responses.push_back(text_response());
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = VerboseTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "compaction-test".to_owned(),
                system: "Inspect before completing.".to_owned(),
                prompt: "Read the workspace until the task is complete.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(8, 8).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop");

        assert_eq!(tools.calls, 6);
        assert!(outcome.compactions > 0);
        assert_eq!(u32::from(outcome.compactions), trace.compactions);
        let requests = provider.requests.lock().expect("requests");
        let last = requests.last().expect("final request");
        assert_eq!(last.messages()[0].role(), Role::System);
        assert_eq!(last.messages()[1].role(), Role::User);
        assert!(last.messages().iter().any(|message| {
            message.role() == Role::User
                && message.content().iter().any(|block| {
                    matches!(block, ContentBlock::Text(text) if text.expose_for_wire().starts_with("<peritus-compaction ") && text.expose_for_wire().contains("source_sha256="))
                })
        }));
        drop(requests);
    });
}
