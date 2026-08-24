use peritus_model_protocol::{Capability, FailureCategory, ModelEvent, WireDialect};
use peritus_provider_core::{
    CancellationToken, FramingLimits, HttpLimits, MemoryByteStream, OwnedModelStream,
};

use super::support::{block_on, chat_profile, fixture, responses_profile};
use crate::stream::CompatibleStream;

async fn collect(name: &str, dialect: WireDialect) -> Vec<peritus_model_protocol::EventEnvelope> {
    let bytes = fixture(name);
    let chunks = bytes.chunks(7).map(<[u8]>::to_vec).collect();
    let body = MemoryByteStream::new(chunks, HttpLimits::PRODUCTION).expect("body");
    let tools = name.contains("tool");
    let capabilities = if tools {
        vec![Capability::Streaming, Capability::UsageDetail, Capability::ToolCalls]
    } else {
        vec![Capability::Streaming, Capability::UsageDetail]
    };
    let profile = match dialect {
        WireDialect::CompatibleResponses => responses_profile(&capabilities),
        WireDialect::CompatibleChatCompletions => chat_profile(&capabilities),
        _ => panic!("test dialect"),
    };
    let stream = CompatibleStream::new(
        Box::new(body),
        FramingLimits::PRODUCTION,
        profile.provider().clone(),
        profile.model().clone(),
        dialect,
        false,
        tools,
        true,
        peritus_model_protocol::ProtocolLimits::PRODUCTION,
        Vec::new(),
    )
    .expect("stream");
    let mut stream = OwnedModelStream::new(stream, CancellationToken::new());
    let mut events = Vec::new();
    while let Some(event) = stream.pull().await.expect("pull") {
        events.push(event);
    }
    events
}

#[test]
fn both_dialects_keep_tool_argument_fragments_as_ordered_text() {
    block_on(async {
        for (name, dialect) in [
            ("responses-tool.sse", WireDialect::CompatibleResponses),
            ("chat-tool.sse", WireDialect::CompatibleChatCompletions),
        ] {
            let events = collect(name, dialect).await;
            assert_eq!(
                events
                    .iter()
                    .filter(|value| matches!(value.event(), ModelEvent::ToolArgumentDelta { .. }))
                    .count(),
                2
            );
            assert!(events.iter().any(|value| matches!(
                value.event(),
                ModelEvent::Finish(peritus_model_protocol::FinishReason::ToolCalls)
            )));
            assert!(
                events.iter().any(|value| matches!(value.event(), ModelEvent::ResponseCompleted))
            );
        }
    });
}

#[test]
fn chat_refusal_has_its_own_item_and_http_success_errors_are_terminal_failures() {
    block_on(async {
        let refusal = collect("chat-refusal.sse", WireDialect::CompatibleChatCompletions).await;
        assert!(refusal.iter().any(|value| matches!(
            value.event(),
            ModelEvent::ItemStarted { kind: peritus_model_protocol::ItemKind::Refusal, .. }
        )));
        assert_eq!(
            refusal
                .iter()
                .filter(|value| matches!(value.event(), ModelEvent::RefusalDelta { .. }))
                .count(),
            2
        );
        for (name, dialect) in [
            ("responses-error-after-http-success.sse", WireDialect::CompatibleResponses),
            ("chat-error-after-http-success.sse", WireDialect::CompatibleChatCompletions),
        ] {
            let events = collect(name, dialect).await;
            assert!(
                events.iter().any(|value| matches!(value.event(), ModelEvent::ResponseFailed(_)))
            );
            assert!(
                !events.iter().any(|value| matches!(value.event(), ModelEvent::ResponseCompleted))
            );
        }
    });
}

#[test]
fn responses_and_chat_fragmented_success_normalize_terminal_usage_and_text() {
    block_on(async {
        for (name, dialect) in [
            ("responses-success.sse", WireDialect::CompatibleResponses),
            ("chat-success.sse", WireDialect::CompatibleChatCompletions),
        ] {
            let events = collect(name, dialect).await;
            assert!(
                events.iter().any(|value| matches!(value.event(), ModelEvent::TextDelta { .. }))
            );
            assert!(events.iter().any(|value| matches!(value.event(), ModelEvent::Usage(_))));
            assert_eq!(
                events
                    .iter()
                    .filter(|value| matches!(value.event(), ModelEvent::ResponseCompleted))
                    .count(),
                1
            );
        }
    });
}

#[test]
fn corrupt_incomplete_and_unknown_critical_streams_fail_closed() {
    block_on(async {
        for (name, dialect) in [
            ("responses-corrupt.sse", WireDialect::CompatibleResponses),
            ("responses-incomplete.sse", WireDialect::CompatibleResponses),
            ("responses-unknown-critical.sse", WireDialect::CompatibleResponses),
            ("chat-corrupt.sse", WireDialect::CompatibleChatCompletions),
            ("chat-incomplete.sse", WireDialect::CompatibleChatCompletions),
            ("chat-unknown-critical.sse", WireDialect::CompatibleChatCompletions),
        ] {
            let events = collect(name, dialect).await;
            assert!(events.iter().any(|value| matches!(
                value.event(),
                ModelEvent::ResponseFailed(failure)
                    if matches!(failure.category(), FailureCategory::MalformedPayload | FailureCategory::IncompleteStream)
            )));
            assert!(
                !events.iter().any(|value| matches!(value.event(), ModelEvent::ResponseCompleted))
            );
        }
    });
}

#[test]
fn responses_unknown_ancillary_is_preserved_bounded() {
    block_on(async {
        let events =
            collect("responses-unknown-ancillary.sse", WireDialect::CompatibleResponses).await;
        assert!(events.iter().any(|value| matches!(
            value.event(),
            ModelEvent::ProviderEvent(extension)
                if extension.name().as_str() == "compatible.ancillary"
        )));
        assert!(events.iter().any(|value| matches!(value.event(), ModelEvent::ResponseCompleted)));
    });
}
