//! Canonical normalized-event persistence and rejection behavior.

use peritus_model_protocol::{
    BoundedText, CacheKey, CacheObservation, CacheStatus, CanonicalJson, EventEnvelope, EventId,
    ExtensionName, FailureCategory, FinishReason, ItemId, ItemKind, JsonBounds, ModelEvent,
    ModelFailure, ModelName, OutcomeCertainty, ProtocolErrorKind, ProtocolLimits,
    ProviderExtension, ProviderName, RateLimitDimension, RateLimitObservation, RateLimitWindow,
    RedactedDiagnostic, ResetTime, ResponseId, Retryability, StreamFragment, ToolCallId, ToolName,
    TransportPhase, UsageCounters, UsageObservation, UsageScope, decode_event_envelope,
    encode_event_envelope,
};
use peritus_types::Sha256Digest;

fn envelope(sequence: u64, event: ModelEvent) -> EventEnvelope {
    EventEnvelope::new(
        sequence,
        Some(sequence + 10),
        Some(EventId::new(format!("event-{sequence}")).expect("event identity")),
        Sha256Digest::new([u8::try_from(sequence).expect("small sequence"); 32]),
        event,
    )
    .expect("event envelope")
}

#[test]
#[allow(clippy::too_many_lines, reason = "one table exercises every closed event tag")]
fn normalized_events_round_trip_canonically_with_nested_observations() {
    let limits = ProtocolLimits::PRODUCTION;
    let events = [
        ModelEvent::ResponseStarted {
            response_id: Some(ResponseId::new("response-1".to_owned()).expect("response")),
            model: Some(ModelName::new("model-1".to_owned()).expect("model")),
        },
        ModelEvent::ResponseIdentity(
            ResponseId::new("response-1".to_owned()).expect("response identity"),
        ),
        ModelEvent::ItemStarted {
            item_id: ItemId::new("item-1".to_owned()).expect("item"),
            index: 0,
            kind: ItemKind::StructuredOutput,
        },
        ModelEvent::TextDelta {
            item_id: ItemId::new("item-1".to_owned()).expect("item"),
            fragment: StreamFragment::new(b"text".to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ReasoningSummaryDelta {
            item_id: ItemId::new("item-2".to_owned()).expect("item"),
            fragment: StreamFragment::new(b"summary".to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ReasoningReplayDelta {
            item_id: ItemId::new("item-2".to_owned()).expect("item"),
            fragment: StreamFragment::new(vec![0, 0xff], limits).expect("fragment"),
        },
        ModelEvent::RefusalDelta {
            item_id: ItemId::new("item-3".to_owned()).expect("item"),
            fragment: StreamFragment::new(b"refusal".to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ToolCallStarted {
            item_id: ItemId::new("item-4".to_owned()).expect("item"),
            call_id: ToolCallId::new("call-1".to_owned()).expect("call"),
            name: ToolName::new("read_file".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: ToolCallId::new("call-1".to_owned()).expect("call"),
            fragment: StreamFragment::new(br#"{"path":"a"}"#.to_vec(), limits).expect("fragment"),
        },
        ModelEvent::ItemCompleted(ItemId::new("item-1".to_owned()).expect("item")),
        ModelEvent::Heartbeat,
        ModelEvent::Usage(UsageObservation::new(
            UsageScope::Final,
            UsageCounters::new(
                Some(11),
                Some(2),
                Some(3),
                Some(5),
                Some(1),
                Some(7),
                Some(16),
                Some(900),
            ),
            Some(
                CanonicalJson::parse(r#"{"cached":true}"#, JsonBounds::value(limits))
                    .expect("usage detail"),
            ),
        )),
        ModelEvent::RateLimit(
            RateLimitObservation::new(vec![
                RateLimitWindow::new(
                    RateLimitDimension::Requests,
                    Some(100),
                    Some(42),
                    Some(ResetTime::AfterMillis(1_500)),
                )
                .expect("rate limit"),
                RateLimitWindow::new(
                    RateLimitDimension::Provider(
                        ExtensionName::new("openai.batch".to_owned()).expect("dimension"),
                    ),
                    None,
                    None,
                    Some(ResetTime::UnixMillis(4_000)),
                )
                .expect("provider rate limit"),
            ])
            .expect("observation"),
        ),
        ModelEvent::Cache(CacheObservation::new(
            CacheStatus::Hit,
            Some(CacheKey::new("opaque-cache-key".to_owned()).expect("cache key")),
            Some(11),
            Some(60),
        )),
        ModelEvent::ProviderEvent(ProviderExtension::new(
            ExtensionName::new("openai.request".to_owned()).expect("extension name"),
            CanonicalJson::parse(r#"{"a":1,"b":2}"#, JsonBounds::value(limits))
                .expect("extension value"),
        )),
        ModelEvent::Finish(FinishReason::Provider(
            BoundedText::new("provider-stop".to_owned(), limits).expect("finish reason"),
        )),
        ModelEvent::ResponseFailed(ModelFailure::new(
            ProviderName::new("provider".to_owned()).expect("provider"),
            FailureCategory::RateLimited,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::ExactResumeOnly,
            Some(429),
            Some(ResponseId::new("response-1".to_owned()).expect("response")),
            Some(1_000),
            RedactedDiagnostic::new(
                "provider.rate_limited".to_owned(),
                Some(10),
                Some(20),
                Some(30),
            )
            .expect("diagnostic"),
        )),
        ModelEvent::ResponseCancelled,
        ModelEvent::ResponseCompleted,
    ];
    for (index, event) in events.into_iter().enumerate() {
        let original = envelope(u64::try_from(index + 1).expect("sequence"), event);
        let bytes = encode_event_envelope(&original, limits).expect("encode");
        let decoded = decode_event_envelope(&bytes, limits).expect("decode");
        assert_eq!(decoded, original);
        assert_eq!(encode_event_envelope(&decoded, limits).expect("re-encode"), bytes);
    }
}

#[test]
fn decoder_rejects_schema_drift_truncation_and_trailing_bytes() {
    let limits = ProtocolLimits::PRODUCTION;
    let canonical = encode_event_envelope(&envelope(1, ModelEvent::Heartbeat), limits)
        .expect("canonical event");

    let mut wrong_schema = canonical.clone();
    wrong_schema[5] = 2;
    assert_eq!(
        decode_event_envelope(&wrong_schema, limits).expect_err("schema drift").kind(),
        ProtocolErrorKind::UnsupportedVersion
    );
    assert!(decode_event_envelope(&canonical[..canonical.len() - 1], limits).is_err());

    let mut trailing = canonical;
    trailing.push(0);
    assert!(decode_event_envelope(&trailing, limits).is_err());
}
