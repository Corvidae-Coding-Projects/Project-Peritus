//! `OpenCode` gateway recordings and provider-specific stream normalization.

use super::client_with_responses;
use crate::tests::support::{block_on, chat_profile, fixture};
use peritus_model_protocol::{Capability, ModelEvent, ProtocolLimits, WireDialect};
use peritus_provider_core::{
    CancellationToken, FramingLimits, HttpLimits, MemoryByteStream, OwnedModelStream,
    connection::{ConnectionStage, verify_provider_connection},
    hosted::HostedService,
};

#[test]
fn live_zen_deepseek_replay_completes_connection_with_nullable_tool_roles() {
    block_on(async {
        let (client, transport) =
            client_with_responses(HostedService::OpenCodeZen, None, |_, step| {
                fixture(&format!("zen-deepseek-live-{step}.sse"))
            });
        let report = verify_provider_connection(&client, CancellationToken::new())
            .await
            .expect("recorded Zen generation, tool call, and tool-result replay");
        assert_eq!(
            report.completed,
            [
                ConnectionStage::Generation,
                ConnectionStage::ToolCalling,
                ConnectionStage::ToolResult
            ]
        );
        assert_eq!(transport.requests.lock().expect("requests").len(), 3);
    });
}

#[test]
fn zen_gateway_batched_choice_fragments_complete_the_tool_round_trip() {
    block_on(async {
        let (client, _) = client_with_responses(HostedService::OpenCodeZen, None, |_, step| {
            if step == 2 {
                fixture("zen-deepseek-batched-choice.sse")
            } else {
                fixture(&format!("zen-deepseek-live-{step}.sse"))
            }
        });
        verify_provider_connection(&client, CancellationToken::new())
            .await
            .expect("Zen gateway fragments from one choice remain one completion");
    });
}

#[test]
fn batched_choice_fragments_remain_service_specific_and_single_choice() {
    block_on(async {
        let bytes = fixture("zen-deepseek-batched-choice.sse");
        let ordinary = hosted_events(bytes.clone(), HostedService::DeepSeek).await;
        assert!(
            ordinary.iter().any(|event| matches!(event.event(), ModelEvent::ResponseFailed(_))),
            "ordinary Chat endpoints retain the one-choice contract"
        );

        let alternate = String::from_utf8(bytes)
            .expect("fixture UTF-8")
            .replacen(
                "{\"index\":0,\"finish_reason\":\"tool_calls\",\"delta\":{}}",
                "{\"index\":1,\"finish_reason\":\"tool_calls\",\"delta\":{}}",
                1,
            )
            .into_bytes();
        let alternate = hosted_events(alternate, HostedService::OpenCodeZen).await;
        assert!(
            alternate.iter().any(|event| matches!(event.event(), ModelEvent::ResponseFailed(_))),
            "Zen batching does not admit an alternate completion choice"
        );
    });
}

async fn hosted_events(
    bytes: Vec<u8>,
    service: HostedService,
) -> Vec<peritus_model_protocol::EventEnvelope> {
    let profile = chat_profile(&[
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::UsageDetail,
        Capability::ReasoningReplay,
    ]);
    let body = MemoryByteStream::new(
        bytes.chunks(11).map(<[u8]>::to_vec).collect(),
        HttpLimits::PRODUCTION,
    )
    .expect("body");
    let stream = crate::stream::CompatibleStream::new(
        Box::new(body),
        FramingLimits::PRODUCTION,
        profile.provider().clone(),
        profile.model().clone(),
        WireDialect::CompatibleChatCompletions,
        false,
        true,
        true,
        ProtocolLimits::PRODUCTION,
        Vec::new(),
    )
    .expect("stream")
    .with_hosted_service(Some(service));
    let mut stream = OwnedModelStream::new(stream, CancellationToken::new());
    let mut events = Vec::new();
    while let Some(event) = stream.pull().await.expect("pull") {
        events.push(event);
    }
    events
}
