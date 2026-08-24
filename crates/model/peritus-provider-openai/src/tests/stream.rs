use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use peritus_model_protocol::{FailureCategory, ModelEvent, ResponseReducer, TerminalOutcome};
use peritus_provider_core::{
    CancellationToken, FramingLimits, HttpLimits, MemoryByteStream, OwnedModelStream,
};

use super::support::{block_on, fixture, profile_minimal};
use crate::stream::{OpenAiStream, metadata::ResponseMetadata};

async fn collect_fixture(
    bytes: &[u8],
    splits: &[usize],
) -> Vec<peritus_model_protocol::EventEnvelope> {
    let mut chunks = Vec::new();
    let mut start = 0;
    for end in splits.iter().copied().filter(|end| *end < bytes.len()) {
        chunks.push(bytes[start..end].to_vec());
        start = end;
    }
    chunks.push(bytes[start..].to_vec());
    let body = MemoryByteStream::new(chunks, HttpLimits::PRODUCTION).expect("body");
    let profile = profile_minimal();
    let stream = OpenAiStream::new(
        Box::new(body),
        FramingLimits::PRODUCTION,
        profile.provider().clone(),
        profile.model().clone(),
        false,
        peritus_model_protocol::ProtocolLimits::PRODUCTION,
        ResponseMetadata::empty(),
        false,
        Arc::new(Mutex::new(BTreeSet::new())),
    );
    let mut stream = OwnedModelStream::new(stream, CancellationToken::new());
    let mut events = Vec::new();
    while let Some(event) = stream.pull().await.expect("normalized pull") {
        events.push(event);
    }
    events
}

#[test]
fn fragmented_success_reduces_to_exact_usage_and_terminal() {
    block_on(async {
        let bytes = fixture("success.sse");
        let events = collect_fixture(&bytes, &[1, 7, 31, 127, bytes.len() - 1]).await;
        assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::TextDelta { .. })));
        assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Usage(_))));
        assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Cache(_))));
        let profile = profile_minimal();
        let mut reducer = ResponseReducer::new(
            profile.provider().clone(),
            peritus_model_protocol::ProtocolLimits::PRODUCTION,
        );
        for event in events {
            reducer.push(event).expect("event reduces");
        }
        assert!(matches!(reducer.terminal(), Some(TerminalOutcome::Succeeded { .. })));
        assert_eq!(reducer.usage_high_water().cached_input_tokens(), Some(2));
    });
}

#[test]
fn corrupt_incomplete_unknown_critical_and_stream_error_fail_closed() {
    block_on(async {
        for fixture in [
            fixture("corrupt.sse"),
            fixture("incomplete.sse"),
            fixture("unknown-critical.sse"),
            fixture("error-after-success.sse"),
        ] {
            let events = collect_fixture(&fixture, &[3, 19]).await;
            let failure = events
                .iter()
                .find_map(|event| match event.event() {
                    ModelEvent::ResponseFailed(failure) => Some(failure),
                    _ => None,
                })
                .expect("explicit failure");
            assert!(matches!(
                failure.category(),
                FailureCategory::MalformedPayload
                    | FailureCategory::IncompleteStream
                    | FailureCategory::Provider
            ));
            assert!(
                !events.iter().any(|event| matches!(event.event(), ModelEvent::ResponseCompleted))
            );
        }
    });
}

#[test]
fn ancillary_unknown_is_bounded_and_tool_fragments_remain_ordered() {
    block_on(async {
        let ancillary_bytes = fixture("unknown-ancillary.sse");
        let ancillary = collect_fixture(&ancillary_bytes, &[2, 11, 53]).await;
        assert!(ancillary.iter().any(|event| matches!(
            event.event(),
            ModelEvent::ProviderEvent(extension)
                if extension.name().as_str() == "openai.ancillary"
                    && extension.value().canonical_bytes()
                        == br#"{"future":"bounded","sequence_number":2,"type":"provider.notice"}"#
        )));
        assert!(
            ancillary.iter().any(|event| matches!(event.event(), ModelEvent::ResponseCompleted))
        );

        let tool_bytes = fixture("tool-reasoning.sse");
        let tool = collect_fixture(&tool_bytes, &[1, 5, 17, 64, 255]).await;
        assert_eq!(
            tool.iter()
                .filter(|event| matches!(event.event(), ModelEvent::ToolArgumentDelta { .. }))
                .count(),
            2
        );
        assert!(
            tool.iter()
                .any(|event| matches!(event.event(), ModelEvent::ReasoningReplayDelta { .. }))
        );
    });
}
