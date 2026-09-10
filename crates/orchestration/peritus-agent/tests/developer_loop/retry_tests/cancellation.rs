//! Attempt cleanup must propagate caller cancellation without cancelling the caller on failure.

use super::*;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Boundary {
    Start,
    Stream,
}

struct PendingProvider {
    profile: ProviderProfile,
    boundary: Boundary,
    observed: Arc<Mutex<Option<CancellationToken>>>,
}

struct PendingStream(Arc<Mutex<Option<CancellationToken>>>);

impl ModelProvider for PendingProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            if matches!(self.boundary, Boundary::Start) {
                *self.observed.lock().expect("attempt token") = Some(cancellation);
                std::future::pending().await
            } else {
                Ok(OwnedModelStream::new(PendingStream(Arc::clone(&self.observed)), cancellation))
            }
        })
    }
}

impl ModelStream for PendingStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            *self.0.lock().expect("attempt token") = Some(cancellation.clone());
            std::future::pending().await
        })
    }
}

#[test]
fn explicit_stop_cancels_pending_start_and_stream_without_retry() {
    block_on(async {
        for boundary in [Boundary::Start, Boundary::Stream] {
            pending_attempt(boundary, true).await;
        }
    });
}

#[test]
fn dropped_loop_cleans_up_pending_start_and_stream_without_cancelling_caller() {
    block_on(async {
        for boundary in [Boundary::Start, Boundary::Stream] {
            pending_attempt(boundary, false).await;
        }
    });
}

async fn pending_attempt(boundary: Boundary, explicit_stop: bool) {
    let observed = Arc::new(Mutex::new(None));
    let provider =
        PendingProvider { profile: profile(), boundary, observed: Arc::clone(&observed) };
    let caller = CancellationToken::new();
    let mut tools = RecordingTool::default();
    let mut trace = RecordingTrace::default();
    let mut operation = Box::pin(DeveloperLoop::run(
        &provider,
        DeveloperLoopRequest {
            request_prefix: "pending-attempt-cancellation".to_owned(),
            system: "Complete the task.".to_owned(),
            prompt: "Return the result.".to_owned(),
            attachments: Vec::new(),
            tools: vec![read_tool()],
            limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
            cancellation: caller.clone(),
        },
        &mut tools,
        &mut trace,
    ));
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            _ = &mut operation => panic!("provider must be pending"),
            () = async {
                while observed.lock().expect("attempt token").is_none() {
                    tokio::task::yield_now().await;
                }
            } => {}
        }
    })
    .await
    .expect("reach pending boundary");
    if explicit_stop {
        let _ = caller.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), &mut operation)
            .await
            .expect("explicit stop is prompt");
        assert!(matches!(result, Err(peritus_agent::DeveloperLoopError::Cancelled)));
    }
    drop(operation);
    assert_eq!(caller.is_cancelled(), explicit_stop);
    assert!(
        observed.lock().expect("attempt token").as_ref().expect("captured token").is_cancelled()
    );
    assert!(trace.retries.is_empty());
    assert_eq!(tools.calls, 0);
}
