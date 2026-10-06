use super::*;
use peritus_agent::{DeveloperLoopError, DeveloperToolExecution};
use peritus_model_protocol::CompletedToolCall;
use std::sync::Arc;

struct WaitingTool {
    entered: Arc<tokio::sync::Notify>,
    available: Arc<tokio::sync::Notify>,
    completed: u32,
}
impl DeveloperToolExecutor for WaitingTool {
    fn execute(
        &mut self,
        _: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        panic!("this host requires awaitable execution")
    }
    fn execute_async<'a>(&'a mut self, call: &'a CompletedToolCall) -> DeveloperToolExecution<'a> {
        Box::pin(async move {
            assert_eq!(call.id().expose_for_wire(), "read-call");
            self.entered.notify_one();
            self.available.notified().await;
            self.completed += 1;
            Ok(DeveloperToolObservation {
                output: CanonicalJson::parse(
                    r#"{"content":"retained proposal completed"}"#,
                    JsonBounds::value(ProtocolLimits::PRODUCTION),
                )?,
                is_error: false,
            })
        })
    }
}

#[test]
fn waiting_tool_keeps_one_proposal_and_the_same_conversation() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let entered = Arc::new(tokio::sync::Notify::new());
        let available = Arc::new(tokio::sync::Notify::new());
        let mut tools =
            WaitingTool { entered: entered.clone(), available: available.clone(), completed: 0 };
        let mut trace = RecordingTrace::default();
        let native_session = tempfile::tempdir().expect("native session namespace");
        let request = DeveloperLoopRequest {
            local_session_directory: Some(native_session.path().to_owned()),
            request_prefix: "retained-tool-wait".to_owned(),
            system: "Keep this conversation".to_owned(),
            prompt: "Read the requested file".to_owned(),
            attachments: Vec::new(),
            tools: vec![read_tool()],
            limits: DeveloperLoopLimits::new(4, 4).expect("test limits"),
            cancellation: CancellationToken::new(),
        };
        let mut pending = Box::pin(DeveloperLoop::run(&provider, request, &mut tools, &mut trace));
        std::future::poll_fn(|context| match pending.as_mut().poll(context) {
            std::task::Poll::Pending => std::task::Poll::Ready(()),
            std::task::Poll::Ready(_) => panic!("resource wait must suspend the tool"),
        })
        .await;
        entered.notified().await;
        assert_eq!(
            provider.requests.lock().expect("requests").len(),
            1,
            "resource waiting must not ask the provider to repeat a proposal"
        );
        available.notify_one();
        let outcome = pending.await;
        let outcome = outcome.expect("resumed developer loop");
        assert_eq!(outcome.tool_calls, 1);
        assert_eq!(outcome.model_turns, 2);
        assert_eq!(tools.completed, 1);
        assert_eq!(trace.observations, 1);
        assert!(trace.retries.is_empty());
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|request| request.local_session_directory() == Some(native_session.path())),
            "waiting must preserve the native provider session binding"
        );
        assert!(requests[1].messages().iter().any(|message| message.role() == Role::Tool
            && message.content().iter().any(|block| matches!(block, ContentBlock::ToolResult(_)))));
        drop(requests);
    });
}
