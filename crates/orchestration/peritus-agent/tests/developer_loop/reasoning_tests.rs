//! Exact reasoning replay through the real D0 tool-result transcript.
use super::*;
use peritus_model_protocol::{
    Capability, CapabilityMatrix, ItemId, ItemKind, ModelEvent, StreamFragment,
};

#[test]
fn developer_loop_preserves_opaque_reasoning_on_the_next_tool_result_request() {
    for opaque in [true, false] {
        block_on(async {
            let original = profile();
            let profile = ProviderProfile::new(
                original.profile_id(),
                original.revision(),
                original.provider().clone(),
                original.model().clone(),
                original.dialect(),
                CapabilityMatrix::new(&[Capability::ToolCalls, Capability::ReasoningReplay], &[])
                    .expect("capabilities"),
                original.provenance(),
                original.limits(),
                original.output_limit_enforcement(),
                original.state_mode(),
                original.resume_kind(),
                original.cancellation_kind(),
            )
            .expect("profile");
            let replay =
                br#"{"service":"deepseek","fields":{"reasoning_content":"opaque thought"}}"#;
            let item = ItemId::new("reasoning-item".to_owned()).expect("item");
            let mut events: Vec<_> =
                tool_response().into_iter().map(|event| event.event().clone()).collect();
            events.splice(
                1..1,
                [
                    ModelEvent::ItemStarted {
                        item_id: item.clone(),
                        index: 1,
                        kind: ItemKind::Reasoning,
                    },
                    if opaque {
                        ModelEvent::ReasoningReplayDelta {
                            item_id: item.clone(),
                            fragment: StreamFragment::new(
                                replay.to_vec(),
                                ProtocolLimits::PRODUCTION,
                            )
                            .expect("replay"),
                        }
                    } else {
                        ModelEvent::ReasoningSummaryDelta {
                            item_id: item.clone(),
                            fragment: StreamFragment::new(
                                b"visible summary".to_vec(),
                                ProtocolLimits::PRODUCTION,
                            )
                            .expect("summary"),
                        }
                    },
                    ModelEvent::ItemCompleted(item),
                ],
            );
            let response = events
                .into_iter()
                .enumerate()
                .map(|(index, event)| {
                    EventEnvelope::new(
                        u64::try_from(index + 1).expect("sequence"),
                        None,
                        None,
                        peritus_types::Sha256Digest::new([1; 32]),
                        event,
                    )
                    .expect("envelope")
                })
                .collect();
            let provider = ScriptedProvider {
                profile,
                responses: Mutex::new(VecDeque::from([response, text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            let mut tools = RecordingTool::default();
            DeveloperLoop::run(
                &provider,
                DeveloperLoopRequest {
                    request_prefix: "reasoning-replay-test".to_owned(),
                    system: "Complete the task.".to_owned(),
                    prompt: "Inspect the file.".to_owned(),
                    attachments: Vec::new(),
                    tools: vec![read_tool()],
                    limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                    cancellation: CancellationToken::new(),
                },
                &mut tools,
                &mut RecordingTrace::default(),
            )
            .await
            .expect("loop");
            let requests = provider.requests.lock().expect("requests");
            assert_eq!(requests.len(), 2);
            assert_eq!(tools.calls, 1);
            assert!(requests[1].negotiated().includes(Capability::ReasoningReplay));
            assert_eq!(requests[1].messages().iter().flat_map(peritus_model_protocol::Message::content).any(|block| matches!(block,ContentBlock::Reasoning(value) if value.opaque_for_wire() == replay)), opaque);
            drop(requests);
        });
    }
}
