//! Progress feedback and complete-batch evidence survive loop termination.

use super::*;

#[test]
fn no_progress_failure_preserves_the_complete_batch_before_stopping() {
    struct StoppedTool(RecordingTool);
    impl DeveloperToolExecutor for StoppedTool {
        fn execute(
            &mut self,
            call: &CompletedToolCall,
        ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
            self.0.execute(call)
        }
        fn continuation_blocker(&self) -> Option<String> {
            Some("inspection-no-progress".to_owned())
        }
    }
    block_on(async {
        let provider = provider(VecDeque::from([tool_response(), text_response()]));
        let mut memory = RecordingContext::default();
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run_with_context(
            &provider,
            request("stopped", 2),
            &mut StoppedTool(RecordingTool::default()),
            &mut trace,
            &mut memory,
        )
        .await;
        assert!(
            matches!(result, Err(DeveloperLoopError::Tool(detail)) if detail == "inspection-no-progress")
        );
        assert_eq!(trace.observations, 1);
        assert_eq!(memory.raw_outputs.len(), 1);
        assert_eq!(memory.batches, 1);
        assert_eq!(provider.requests.lock().unwrap().len(), 1);
    });
}

#[test]
fn progress_warning_reaches_the_next_provider_turn_and_ignored_warning_still_stops() {
    #[derive(Default)]
    struct WarningTool {
        tool: RecordingTool,
        warning_pending: bool,
        warned: bool,
    }
    impl DeveloperToolExecutor for WarningTool {
        fn execute(
            &mut self,
            call: &CompletedToolCall,
        ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
            let observation = self.tool.execute(call)?;
            self.warning_pending = !self.warned;
            Ok(observation)
        }
        fn take_progress_feedback(&mut self) -> Option<String> {
            if !std::mem::take(&mut self.warning_pending) {
                return None;
            }
            self.warned = true;
            Some("inspection warning: use the existing evidence".to_owned())
        }
        fn continuation_blocker(&self) -> Option<String> {
            (self.warned && !self.warning_pending).then(|| "inspection-no-progress".to_owned())
        }
    }
    block_on(async {
        for ignore_warning in [false, true] {
            let next = if ignore_warning { batch_tool_response() } else { text_response() };
            let provider = ScriptedProvider {
                profile: parallel_profile(),
                responses: Mutex::new(VecDeque::from([batch_tool_response(), next])),
                requests: Mutex::new(Vec::new()),
            };
            let mut memory = RecordingContext::default();
            let mut trace = RecordingTrace::default();
            let mut tools = WarningTool::default();
            let result = DeveloperLoop::run_with_context(
                &provider,
                request("batched-warning", 3),
                &mut tools,
                &mut trace,
                &mut memory,
            )
            .await;
            if ignore_warning {
                assert!(
                    matches!(result, Err(DeveloperLoopError::Tool(detail)) if detail == "inspection-no-progress")
                );
            } else {
                assert_eq!(result.expect("the provider can act on the warning").tool_calls, 2);
            }
            let requests = provider.requests.lock().unwrap();
            assert_eq!(requests.len(), 2);
            assert!(requests[1].messages().iter().any(|message| {
                message.role() == Role::User && message.content().iter().any(|block| {
                    matches!(block, ContentBlock::Text(text) if text.expose_for_wire() == "inspection warning: use the existing evidence")
                })
            }));
            assert_eq!(
                requests[1]
                    .messages()
                    .iter()
                    .flat_map(Message::content)
                    .filter(|block| matches!(block, ContentBlock::ToolResult(_)))
                    .count(),
                2
            );
            drop(requests);
            assert_eq!(memory.batches, if ignore_warning { 2 } else { 1 });
            assert_eq!(memory.raw_outputs.len(), if ignore_warning { 4 } else { 2 });
            assert_eq!(trace.observations, tools.tool.calls);
        }
    });
}
