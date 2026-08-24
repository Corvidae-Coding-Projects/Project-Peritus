//! Fragmented official-shape SSE normalization and fail-closed tests.

use peritus_model_protocol::{FailureCategory, FinishReason, ModelEvent};
use peritus_provider_core::{CancellationToken, HttpHeaders, ModelStream};

use super::AnthropicStream;
use crate::test_support::{block_on, profile, response};

fn events(fixture: &[u8], fragment_bytes: bool) -> Vec<peritus_model_protocol::EventEnvelope> {
    let chunks = if fragment_bytes {
        fixture.iter().map(|byte| vec![*byte]).collect()
    } else {
        vec![fixture.to_vec()]
    };
    let response = response(
        200,
        &[
            ("request-id", "req_fixture"),
            ("anthropic-ratelimit-requests-limit", "100"),
            ("anthropic-ratelimit-requests-remaining", "99"),
        ],
        chunks,
    );
    let mut stream = AnthropicStream::new(
        response,
        profile().provider().clone(),
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
fn byte_fragmented_utf8_text_citations_usage_and_metadata_stay_ordered() {
    let events = events(&crate::test_support::fixture("text.sse"), true);
    assert!(events.windows(2).all(|pair| pair[0].sequence() + 1 == pair[1].sequence()));
    assert!(matches!(
        events.first().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseStarted { .. })
    ));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::RateLimit(_))));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Cache(_))));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::Heartbeat)));
    assert!(events.iter().any(|event| matches!(event.event(), ModelEvent::ProviderEvent(_))));
    assert!(events.iter().any(|event| matches!(
        event.event(),
        ModelEvent::TextDelta { fragment, .. } if fragment.expose() == "héllo".as_bytes()
    )));
    assert!(matches!(
        events.last().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseCompleted)
    ));
}

#[test]
fn fragmented_tool_arguments_and_thinking_signature_replay_are_preserved() {
    let events = events(&crate::test_support::fixture("tool_thinking.sse"), true);
    let tool_fragments = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::ToolArgumentDelta { fragment, .. } => Some(fragment.expose()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tool_fragments, [b"{\"city\":".as_slice(), b"\"Paris\"}".as_slice()]);
    assert!(events.iter().any(|event| matches!(
        event.event(),
        ModelEvent::ReasoningReplayDelta { fragment, .. }
            if core::str::from_utf8(fragment.expose()).expect("replay UTF-8")
                == r#"{"signature":"opaque-signature","type":"thinking"}"#
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event.event(), ModelEvent::Finish(FinishReason::ToolCalls)))
    );
}

#[test]
fn corrupt_incomplete_and_unknown_critical_streams_fail_closed() {
    for name in ["corrupt.sse", "incomplete.sse", "unknown_critical.sse"] {
        let events = events(&crate::test_support::fixture(name), true);
        assert!(matches!(
            events.last().map(peritus_model_protocol::EventEnvelope::event),
            Some(ModelEvent::ResponseFailed(_))
        ));
    }
    let incomplete = events(&crate::test_support::fixture("incomplete.sse"), false);
    let Some(ModelEvent::ResponseFailed(failure)) =
        incomplete.last().map(peritus_model_protocol::EventEnvelope::event)
    else {
        panic!("incomplete terminal")
    };
    assert_eq!(failure.category(), FailureCategory::IncompleteStream);
    assert!(!format!("{failure:?}").contains("partial-secret"));
}

#[test]
fn ancillary_unknown_is_preserved_but_http_success_error_is_terminal() {
    let ancillary = events(&crate::test_support::fixture("unknown_ancillary.sse"), false);
    assert!(ancillary.iter().any(|event| matches!(event.event(), ModelEvent::ProviderEvent(_))));
    assert!(matches!(
        ancillary.last().map(peritus_model_protocol::EventEnvelope::event),
        Some(ModelEvent::ResponseCompleted)
    ));

    let errors = events(&crate::test_support::fixture("error_after_success.sse"), false);
    let Some(ModelEvent::ResponseFailed(failure)) =
        errors.last().map(peritus_model_protocol::EventEnvelope::event)
    else {
        panic!("error terminal")
    };
    assert_eq!(failure.category(), FailureCategory::TransientProvider);
    assert!(!format!("{failure:?}").contains("sensitive upstream details"));
}

#[test]
fn cancellation_discards_buffered_output_and_emits_only_cancellation() {
    let response = response(200, &[], vec![crate::test_support::fixture("text.sse")]);
    let mut stream = AnthropicStream::new(
        response,
        profile().provider().clone(),
        peritus_provider_core::FramingLimits::PRODUCTION,
    )
    .expect("stream");
    let cancellation = CancellationToken::new();
    assert!(cancellation.cancel());
    let event = block_on(stream.next(&cancellation)).expect("event").expect("terminal");
    assert!(matches!(event.event(), ModelEvent::ResponseCancelled));
}

#[test]
fn empty_headers_do_not_create_metadata() {
    let state =
        super::state::NormalizeState::new(profile().provider().clone(), &HttpHeaders::empty())
            .expect("state");
    assert!(!state.has_observed_semantics());
}
