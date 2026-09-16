//! Lifecycle projection and terminal no-progress retention regressions.

use super::*;

#[test]
fn executor_observes_the_exact_selected_view_after_context_assembly() {
    #[derive(Default)]
    struct ContextTool {
        tool: RecordingTool,
        views: Vec<Vec<String>>,
    }
    fn result_ids(messages: &[Message]) -> Vec<String> {
        messages
            .iter()
            .flat_map(Message::content)
            .filter_map(|block| match block {
                ContentBlock::ToolResult(result) => {
                    Some(result.call_id().expose_for_wire().to_owned())
                }
                _ => None,
            })
            .collect()
    }
    impl DeveloperToolExecutor for ContextTool {
        fn observe_model_context(
            &mut self,
            messages: &[Message],
        ) -> Result<(), DeveloperLoopError> {
            self.views.push(result_ids(messages));
            Ok(())
        }
        fn execute(
            &mut self,
            call: &CompletedToolCall,
        ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
            self.tool.execute(call)
        }
    }
    block_on(async {
        for omit_tool_history in [false, true] {
            let provider = ScriptedProvider {
                profile: parallel_profile(),
                responses: Mutex::new(VecDeque::from([batch_tool_response(), text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            let mut tools = ContextTool::default();
            let mut trace = RecordingTrace::default();
            let mut memory = RecordingContext { omit_tool_history, ..Default::default() };
            DeveloperLoop::run_with_context(
                &provider,
                request("selected-view", 2),
                &mut tools,
                &mut trace,
                &mut memory,
            )
            .await
            .unwrap();
            let requests = provider.requests.lock().unwrap();
            assert_eq!(tools.views.len(), 2);
            for (view, request) in tools.views.iter().zip(requests.iter()) {
                assert_eq!(*view, result_ids(request.messages()));
            }
            drop(requests);
            assert!(tools.views[0].is_empty());
            assert_eq!(tools.views[1].len(), if omit_tool_history { 0 } else { 2 });
            assert_eq!(trace.observations, 2);
            assert_eq!(memory.raw_outputs.len(), 2, "eviction does not remove durable evidence");
        }
    });
}

#[test]
fn executor_context_failure_prevents_provider_dispatch() {
    struct FailedContextTool(RecordingTool);
    impl DeveloperToolExecutor for FailedContextTool {
        fn observe_model_context(&mut self, _: &[Message]) -> Result<(), DeveloperLoopError> {
            Err(DeveloperLoopError::Tool("context bookkeeping failed".to_owned()))
        }
        fn execute(
            &mut self,
            call: &CompletedToolCall,
        ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
            self.0.execute(call)
        }
    }
    block_on(async {
        let provider = provider(VecDeque::from([text_response()]));
        let mut memory = RecordingContext::default();
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run_with_context(
            &provider,
            request("context-failure", 1),
            &mut FailedContextTool(RecordingTool::default()),
            &mut trace,
            &mut memory,
        )
        .await;
        assert!(
            matches!(result, Err(DeveloperLoopError::Tool(detail)) if detail == "context bookkeeping failed")
        );
        assert!(provider.requests.lock().unwrap().is_empty());
        assert_eq!(trace.observations, 0);
        assert!(memory.raw_outputs.is_empty());
    });
}

#[test]
fn required_tool_retry_dispatches_its_exact_checkpointed_view() {
    block_on(async {
        let provider = provider(VecDeque::from([
            fixtures::recoverable_failure_response(),
            tool_response(),
            text_response(),
        ]));
        let mut memory = RecordingContext::default();
        let mut request = request("retry-view", 2);
        request.limits = request.limits.with_max_attempts_per_turn(3).expect("attempt limit");
        DeveloperLoop::run_with_context(
            &provider,
            request,
            &mut GroundingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await
        .expect("required tool retry recovers");

        let requests = provider.requests.lock().expect("requests").clone();
        assert_eq!(requests.len(), 3);
        assert_eq!(memory.checkpoint_history.len(), requests.len());
        for (request, checkpoint) in requests.iter().zip(&memory.checkpoint_history) {
            assert_eq!(request.messages(), checkpoint);
        }
        assert!(requests[1].messages()[0].content().iter().any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("CURRENT PROVIDER RETRY"))
        }));
    });
}

#[test]
fn retry_that_exceeds_the_active_model_input_limit_is_not_published_or_dispatched() {
    block_on(async {
        let probe = provider(VecDeque::from([fixtures::recoverable_failure_response()]));
        let mut memory = RecordingContext::default();
        let result = DeveloperLoop::run_with_context(
            &probe,
            request("retry-capacity", 2),
            &mut GroundingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await;
        assert!(result.is_err());
        let requests = probe.requests.lock().expect("probe requests").clone();
        assert_eq!(requests.len(), 1);
        let capacity = peritus_agent::estimate_developer_request_tokens(
            requests[0].messages(),
            requests[0].tools(),
        );
        drop(requests);

        let mut provider = provider(VecDeque::from([
            fixtures::recoverable_failure_response(),
            tool_response(),
            text_response(),
        ]));
        let base = profile();
        let limits = base.limits();
        provider.profile = ProviderProfile::new(
            base.profile_id(),
            base.revision(),
            base.provider().clone(),
            base.model().clone(),
            base.dialect(),
            base.capabilities(),
            base.provenance(),
            peritus_model_protocol::ModelLimits::new(
                capacity,
                limits.max_output_tokens(),
                limits.max_tools(),
                limits.max_parallel_tool_calls(),
                limits.max_inline_media_bytes(),
            )
            .expect("active model capacity"),
            base.output_limit_enforcement(),
            base.state_mode(),
            base.resume_kind(),
            base.cancellation_kind(),
        )
        .expect("active model profile");
        let mut memory = RecordingContext::default();
        let mut request = request("retry-capacity", 2);
        request.limits = request.limits.with_max_attempts_per_turn(3).expect("attempt limit");
        let result = DeveloperLoop::run_with_context(
            &provider,
            request,
            &mut GroundingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await;
        assert!(
            matches!(result, Err(DeveloperLoopError::Context(detail)) if detail.contains("exceed provider limit"))
        );
        assert_eq!(provider.requests.lock().expect("requests").len(), 1);
        assert_eq!(memory.checkpoint_history.len(), 1);
    });
}

#[test]
fn failed_invocation_retains_observations_and_a_new_provider_requires_fresh_grounding() {
    block_on(async {
        let first = provider(VecDeque::from([tool_response()]));
        let mut memory = RecordingContext::default();
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run_with_context(
            &first,
            request("first", 1),
            &mut RecordingTool::default(),
            &mut trace,
            &mut memory,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::LimitExceeded)));
        assert_eq!(memory.batches, 1);
        assert!(memory.raw_outputs[0].contains("answer()"));

        let mut second = provider(VecDeque::from([tool_response(), text_response()]));
        second.profile = fixtures::caching_profile();
        let mut tools = GroundingTool::default();
        DeveloperLoop::run_with_context(
            &second,
            request("second", 2),
            &mut tools,
            &mut trace,
            &mut memory,
        )
        .await
        .expect("continued invocation");
        let requests = second.requests.lock().expect("requests");
        assert!(matches!(requests[0].tool_choice(), ToolChoice::Specific(_)));
        let policies: Vec<_> = requests
            .iter()
            .map(|request| {
                let ContentBlock::Text(policy) = &request.messages()[0].content()[0] else {
                    panic!("missing policy");
                };
                policy.expose_for_wire()
            })
            .collect();
        assert!(policies[0].contains("provider_step=1; required_tool=workspace_read"));
        assert!(policies[1].contains("provider_step=2; required_tool=none"));
        assert!(
            policies
                .iter()
                .all(|policy| policy.matches("CURRENT HOST INVOCATION STATE").count() == 1)
        );
        assert!(requests[0].messages().iter().any(|message| message.content().iter().any(|block| {
            matches!(block, ContentBlock::ToolResult(result) if result.output().to_wire_string().contains("answer()"))
        })));
        drop(requests);
        assert_eq!(tools.calls, 1, "only the new provider's call executes");
        assert_eq!(memory.invocations, ["first", "second"]);
        assert!(memory.view.last().expect("terminal message").content().iter().any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire() == "implementation inspected")
        }));
    });
}

#[test]
fn stale_local_invocation_policy_stops_before_publication_or_provider_dispatch() {
    block_on(async {
        let provider = provider(VecDeque::from([text_response()]));
        let mut memory = RecordingContext { ignore_invocation_policy: true, ..Default::default() };
        let result = DeveloperLoop::run_with_context(
            &provider,
            request("stale", 1),
            &mut RecordingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await;
        assert!(
            matches!(result, Err(DeveloperLoopError::Context(detail)) if detail.contains("current host invocation policy"))
        );
        assert!(provider.requests.lock().unwrap().is_empty());
        assert_eq!(memory.generations, 0);
    });
}

#[path = "local_context_lifecycle/progress.rs"]
mod progress;
