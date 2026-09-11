//! Current invocation prerequisites across rejected completion and successful grounding.

use super::*;

#[test]
fn developer_loop_continues_an_early_terminal_in_the_same_grounding_session() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([
                text_response(),
                tool_response(),
                text_response(),
            ])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = GroundingTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "grounding-recovery-test".to_owned(),
                system: "Inspect before completing.".to_owned(),
                prompt: "Read src/lib.rs and report.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("early terminal recovers");

        assert_eq!(outcome.text, "implementation inspected");
        assert_eq!(outcome.model_turns, 3);
        assert_eq!(outcome.tool_calls, 1);
        assert_eq!(tools.calls, 1);
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 3);
        assert!(matches!(
            requests[0].tool_choice(),
            ToolChoice::Specific(name) if name.as_str() == "workspace_read"
        ));
        assert_eq!(requests[0].parallel_tool_policy(), ParallelToolPolicy::Disabled);
        assert!(matches!(
            requests[1].tool_choice(),
            ToolChoice::Specific(name) if name.as_str() == "workspace_read"
        ));
        assert!(matches!(requests[2].tool_choice(), ToolChoice::Auto));
        for (index, request) in requests.iter().enumerate() {
            let ContentBlock::Text(policy) = &request.messages()[0].content()[0] else {
                panic!("missing live host policy");
            };
            let policy = policy.expose_for_wire();
            assert_eq!(policy.matches("CURRENT HOST INVOCATION STATE").count(), 1);
            assert!(policy.contains(&format!("provider_step={}", index + 1)));
            assert!(policy.contains(if index == 2 {
                "required_tool=none"
            } else {
                "required_tool=workspace_read"
            }));
            if index > 0 {
                assert!(policy.contains("continuation of the SAME host invocation"));
            }
        }
        assert!(requests[1].messages().iter().any(|message| {
            message.role() == Role::User
                && message.content().iter().any(|block| {
                    matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("cannot accept that terminal response yet"))
                })
        }));
        drop(requests);
    });
}
