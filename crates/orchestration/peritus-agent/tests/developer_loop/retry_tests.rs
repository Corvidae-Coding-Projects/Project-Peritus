//! Product-loop retry timing, cancellation, and terminal-preservation regressions.

use std::{collections::VecDeque, sync::Mutex, time::Duration};

use fixtures::{
    empty_response, nonretryable_safety_failure_response, profile, read_tool,
    recoverable_failure_response, text_response,
};

use super::*;

#[test]
fn developer_loop_retries_a_recoverable_malformed_provider_turn() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([
                recoverable_failure_response(),
                empty_response(),
                text_response(),
            ])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "recovery-test".to_owned(),
                system: "Complete the task.".to_owned(),
                prompt: "Return the result.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop recovers");

        assert_eq!(outcome.text, "implementation inspected");
        assert_eq!(outcome.model_turns, 1);
        assert_eq!(outcome.retries, 2);
        assert_eq!(provider.requests.lock().expect("requests").len(), 3);
        assert_eq!(trace.retries.len(), 2);
        assert_eq!(
            trace.retries[0].reason(),
            peritus_agent::DeveloperRetryReason::RetryableProviderResponse
        );
        assert_eq!(trace.retries[0].retry_after_millis(), Some(350));
        assert!(trace.retries[0].delay_millis() >= 350);
        assert_eq!(trace.retries[1].reason(), peritus_agent::DeveloperRetryReason::EmptyResponse);
        assert!(trace.retries[1].delay_millis() >= trace.retries[0].delay_millis());
    });
}

#[test]
fn developer_loop_cancels_during_a_planned_retry_wait() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([recoverable_failure_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let cancellation = CancellationToken::new();
        let signal = cancellation.clone();
        let cancel = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = signal.cancel();
        });
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "cancel-retry-test".to_owned(),
                system: "Complete the task.".to_owned(),
                prompt: "Return the result.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation,
            },
            &mut tools,
            &mut trace,
        )
        .await;
        cancel.await.expect("cancellation task");

        assert!(matches!(result, Err(peritus_agent::DeveloperLoopError::Cancelled)));
        assert_eq!(trace.retries.len(), 1);
        assert_eq!(provider.requests.lock().expect("requests").len(), 1);
    });
}

#[test]
fn developer_loop_preserves_a_nonretryable_provider_terminal() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([nonretryable_safety_failure_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "terminal-failure-test".to_owned(),
                system: "Complete the task.".to_owned(),
                prompt: "Return the result.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await;

        assert!(matches!(
            result,
            Err(peritus_agent::DeveloperLoopError::ProviderTerminal {
                category: peritus_model_protocol::FailureCategory::Safety,
                ref diagnostic_code,
                ..
            }) if diagnostic_code == "scripted.safety"
        ));
        assert!(trace.retries.is_empty());
    });
}
