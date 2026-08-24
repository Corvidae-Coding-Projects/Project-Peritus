//! Fragmented dual-dialect stream normalization and fail-closed tests.

use peritus_model_protocol::{FailureCategory, FinishReason, ModelEvent, WireDialect};
use peritus_provider_core::{CancellationToken, ModelStream};

use super::GoogleStream;
use crate::test_support::{block_on, fixture, profile, response};

fn events(
    name: &str,
    dialect: WireDialect,
    fragmented: bool,
) -> Vec<peritus_model_protocol::EventEnvelope> {
    let bytes = fixture(name);
    let chunks =
        if fragmented { bytes.iter().map(|byte| vec![*byte]).collect() } else { vec![bytes] };
    let response = response(
        200,
        &[("content-type", "text/event-stream"), ("x-goog-request-id", "fixture")],
        chunks,
    );
    let mut stream = GoogleStream::new(
        response,
        profile(dialect).provider().clone(),
        dialect,
        false,
        peritus_provider_core::FramingLimits::PRODUCTION,
    )
    .expect("stream");
    block_on(async {
        let cancellation = CancellationToken::new();
        let mut events = Vec::new();
        while let Some(event) = stream.next(&cancellation).await.expect("normalized event") {
            events.push(event);
        }
        events
    })
}

#[test]
fn fragmented_interactions_text_usage_cache_and_metadata_are_ordered() {
    let events = events("interactions_success.sse", WireDialect::GeminiInteractionsV1, true);
    assert!(events.windows(2).all(|pair| pair[0].sequence() + 1 == pair[1].sequence()));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Cache(_))));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Usage(_))));
    assert!(events.iter().any(|event| matches!(
        event.event(), ModelEvent::TextDelta { fragment, .. } if fragment.expose() == "héllo".as_bytes()
    )));
    assert!(matches!(
        events.last().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseCompleted)
    ));
}

#[test]
fn generate_text_tool_thinking_signature_and_final_usage_normalize() {
    let text = events("generate_success.sse", WireDialect::GeminiGenerateContentV1, true);
    assert!(matches!(
        text.last().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseCompleted)
    ));
    assert!(text.iter().any(|event| matches!(event.event(), ModelEvent::Usage(usage) if usage.scope() == peritus_model_protocol::UsageScope::Final)));
    let tool = events("generate_tool_thinking.sse", WireDialect::GeminiGenerateContentV1, true);
    assert!(tool.iter().any(|event| matches!(event.event(), ModelEvent::ReasoningReplayDelta { fragment, .. } if core::str::from_utf8(fragment.expose()).expect("UTF-8") == r#"{"thoughtSignature":"opaque-google-signature"}"#)));
    assert!(
        tool.iter()
            .any(|event| matches!(event.event(), ModelEvent::Finish(FinishReason::ToolCalls)))
    );
}

#[test]
fn tool_argument_fragments_close_only_after_complete_json() {
    let events = events("interactions_tool_thinking.sse", WireDialect::GeminiInteractionsV1, true);
    let fragments = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::ToolArgumentDelta { fragment, .. } => Some(fragment.expose()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fragments, [b"{\"city\":".as_slice(), b"\"Paris\"}".as_slice()]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event(), ModelEvent::Finish(FinishReason::ToolCalls)))
    );
}

#[test]
fn corrupt_incomplete_and_unknown_critical_fail_closed() {
    for name in ["corrupt.sse", "incomplete.sse", "unknown_critical.sse"] {
        let events = events(name, WireDialect::GeminiInteractionsV1, true);
        assert!(
            matches!(
                events.last().map(peritus_model_protocol::EventEnvelope::event),
                Some(ModelEvent::ResponseFailed(_))
            ),
            "{name}"
        );
    }
    let incomplete = events("incomplete.sse", WireDialect::GeminiInteractionsV1, false);
    let Some(ModelEvent::ResponseFailed(failure)) =
        incomplete.last().map(peritus_model_protocol::EventEnvelope::event)
    else {
        panic!("failure")
    };
    assert_eq!(failure.category(), FailureCategory::IncompleteStream);
    assert!(!format!("{failure:?}").contains("partial-secret"));
}

#[test]
fn ancillary_unknown_is_preserved_and_sse_error_is_terminal() {
    let ancillary = events("unknown_ancillary.sse", WireDialect::GeminiInteractionsV1, false);
    assert!(ancillary.iter().any(|event| matches!(event.event(), ModelEvent::ProviderEvent(_))));
    let errors = events("error_after_success.sse", WireDialect::GeminiInteractionsV1, false);
    let Some(ModelEvent::ResponseFailed(failure)) =
        errors.last().map(peritus_model_protocol::EventEnvelope::event)
    else {
        panic!("failure")
    };
    assert_eq!(failure.category(), FailureCategory::TransientProvider);
    assert!(!format!("{failure:?}").contains("sensitive upstream details"));

    let generate_ancillary =
        events("generate_unknown_ancillary.sse", WireDialect::GeminiGenerateContentV1, false);
    assert!(
        generate_ancillary
            .iter()
            .any(|event| matches!(event.event(), ModelEvent::ProviderEvent(_)))
    );
    let generate_error =
        events("generate_error_after_success.sse", WireDialect::GeminiGenerateContentV1, false);
    let Some(ModelEvent::ResponseFailed(failure)) =
        generate_error.last().map(peritus_model_protocol::EventEnvelope::event)
    else {
        panic!("failure")
    };
    assert_eq!(failure.category(), FailureCategory::TransientProvider);
    assert!(!format!("{failure:?}").contains("sensitive generate detail"));
}

#[test]
fn cancellation_discards_buffered_provider_output() {
    let response = response(200, &[], vec![fixture("interactions_success.sse")]);
    let mut stream = GoogleStream::new(
        response,
        profile(WireDialect::GeminiInteractionsV1).provider().clone(),
        WireDialect::GeminiInteractionsV1,
        false,
        peritus_provider_core::FramingLimits::PRODUCTION,
    )
    .expect("stream");
    let cancellation = CancellationToken::new();
    assert!(cancellation.cancel());
    let event = block_on(stream.next(&cancellation)).expect("event").expect("terminal");
    assert!(matches!(event.event(), ModelEvent::ResponseCancelled));
}
