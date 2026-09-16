//! A dropped invocation cannot erase usage accepted before the next streaming wait.
use super::*;
use peritus_agent::DeveloperAccountingEvent;
use peritus_model_protocol::{ModelEvent, UsageCounters, UsageObservation, UsageScope};
use std::task::Poll;

#[test]
fn ordinary_developer_requests_negotiate_usage_when_supported() {
    use peritus_model_protocol::Capability;
    block_on(async {
        for usage_supported in [true, false] {
            let mut capabilities = vec![Capability::ToolCalls];
            if usage_supported {
                capabilities.push(Capability::UsageDetail);
            }
            let provider = ScriptedProvider {
                profile: fixtures::profile_with_capabilities(&capabilities),
                responses: Mutex::new(VecDeque::from([text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            DeveloperLoop::run(
                &provider,
                DeveloperLoopRequest {
                    request_prefix: "usage-negotiation".to_owned(),
                    system: "Answer the question.".to_owned(),
                    prompt: "Say ok.".to_owned(),
                    attachments: Vec::new(),
                    tools: vec![read_tool()],
                    limits: DeveloperLoopLimits::new(2, 2).expect("limits"),
                    cancellation: CancellationToken::new(),
                },
                &mut RecordingTool::default(),
                &mut RecordingTrace::default(),
            )
            .await
            .expect("ordinary developer request");
            let requests = provider.requests.lock().expect("requests");
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].negotiated().includes(Capability::UsageDetail), usage_supported);
            drop(requests);
        }
    });
}

struct SuspendedProvider(ProviderProfile);
struct SuspendedStream(VecDeque<EventEnvelope>);

impl ModelStream for SuspendedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if let Some(event) = self.0.pop_front() {
                Ok(Some(event))
            } else {
                std::future::pending().await
            }
        })
    }
}

impl ModelProvider for SuspendedProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.0
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            let events = [
                ModelEvent::ResponseStarted { response_id: None, model: None },
                ModelEvent::Usage(UsageObservation::new(
                    UsageScope::Cumulative,
                    UsageCounters::new(Some(10), None, None, Some(2), None, None, Some(12), None),
                    None,
                )),
            ];
            let events = events
                .into_iter()
                .enumerate()
                .map(|(index, event)| {
                    EventEnvelope::new(
                        index as u64 + 1,
                        None,
                        None,
                        peritus_types::Sha256Digest::new([1; 32]),
                        event,
                    )
                    .unwrap()
                })
                .collect();
            Ok(OwnedModelStream::new(SuspendedStream(events), cancellation))
        })
    }
}

#[test]
fn dropping_an_inflight_response_retains_accepted_accounting_without_a_terminal() {
    block_on(async {
        let provider = SuspendedProvider(profile());
        let mut trace = RecordingTrace::default();
        let mut tools = RecordingTool::default();
        let request = DeveloperLoopRequest {
            request_prefix: "dropped-accounting".to_owned(),
            system: "Inspect the workspace".to_owned(),
            prompt: "Read the file".to_owned(),
            attachments: Vec::new(),
            tools: vec![read_tool()],
            limits: DeveloperLoopLimits::new(2, 2).unwrap(),
            cancellation: CancellationToken::new(),
        };
        let mut invocation =
            Box::pin(DeveloperLoop::run(&provider, request, &mut tools, &mut trace));
        let state = std::future::poll_fn(|cx| Poll::Ready(invocation.as_mut().poll(cx))).await;
        assert!(state.is_pending(), "the provider is suspended without a terminal");
        drop(invocation);
        assert_eq!(trace.accounting.len(), 2, "no terminal callback was needed");
        assert_eq!(trace.accounting[0], DeveloperAccountingEvent::ModelRequest { retry: false });
        assert!(
            matches!(trace.accounting[1], DeveloperAccountingEvent::Usage(usage) if usage.total_tokens() == Some(12))
        );
        assert_eq!(tools.calls, 0);
    });
}
