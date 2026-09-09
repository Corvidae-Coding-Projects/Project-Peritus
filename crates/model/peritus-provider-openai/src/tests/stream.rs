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
    collect_output(bytes, splits, false).await
}

async fn collect_output(
    bytes: &[u8],
    splits: &[usize],
    structured: bool,
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
        structured,
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
fn structured_output_healing_does_not_rewrite_ordinary_prose() {
    block_on(async {
        let raw =
            String::from_utf8(fixture("success.sse")).unwrap().replace("hello", "{answer:42,}");
        for structured in [true, false] {
            let events = collect_output(raw.as_bytes(), &[11, 33], structured).await;
            let text: Vec<_> = events
                .iter()
                .filter_map(|event| match event.event() {
                    ModelEvent::TextDelta { fragment, .. } => Some(fragment.expose()),
                    _ => None,
                })
                .flatten()
                .copied()
                .collect();
            assert_eq!(
                text,
                if structured { br#"{"answer":42}"#.as_slice() } else { b"{answer:42,}" }
            );
            assert_eq!(events.iter().any(|event| matches!(event.event(), ModelEvent::ProviderEvent(value) if value.name().as_str() == "peritus.response_healing")), structured);
        }
    });
}

#[test]
fn native_healing_preserves_terminal_binding_and_replays_audited_tool_calls() {
    block_on(async {
        let raw = String::from_utf8(fixture("tool-reasoning.sse")).unwrap();
        for (replacement, repaired) in [(r#"\"42\",}"#, true), (r#"\"42\""#, false)] {
            let bytes = raw.replace(r#"\"42\"}"#, replacement);
            assert_ne!(bytes, raw);
            let events = collect_fixture(bytes.as_bytes(), &[1, 17, 90]).await;
            let audit = events.iter().find_map(|event| match event.event() {
                ModelEvent::ProviderEvent(value)
                    if value.name().as_str() == "peritus.response_healing" =>
                {
                    Some(value)
                }
                _ => None,
            });
            assert_eq!(audit.is_some(), repaired);
            if repaired {
                let mut reducer = ResponseReducer::new(
                    profile_minimal().provider().clone(),
                    peritus_model_protocol::ProtocolLimits::PRODUCTION,
                );
                for event in events {
                    reducer.push(event).unwrap();
                }
                assert!(matches!(reducer.terminal(), Some(TerminalOutcome::RequiresAction { .. })));
            } else {
                assert!(
                    events
                        .iter()
                        .any(|event| matches!(event.event(), ModelEvent::ResponseFailed(_)))
                );
                assert!(
                    !events
                        .iter()
                        .any(|event| matches!(event.event(), ModelEvent::ToolArgumentDelta { .. }))
                );
            }
        }
        let changed_terminal = raw.replacen(
            r#""arguments":"{\"id\":\"42\"}""#,
            r#""arguments":"{\"id\":\"43\",}""#,
            1,
        );
        let events = collect_fixture(changed_terminal.as_bytes(), &[7]).await;
        assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::ResponseFailed(_))));
    });
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
