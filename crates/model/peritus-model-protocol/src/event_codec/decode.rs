use peritus_codec::CanonicalReader;
use peritus_types::Sha256Digest;

use super::primitive::{
    MAGIC, MAX_CANONICAL_EVENT_BYTES, codec_limits, invalid, option_u16, option_u64, read_codec,
    unknown,
};
use crate::{
    BoundedText, CacheKey, CacheObservation, CacheStatus, CanonicalJson, EventEnvelope, EventId,
    ExtensionName, FailureCategory, FinishReason, ItemId, ItemKind, JsonBounds, ModelEvent,
    ModelFailure, ModelName, OutcomeCertainty, ProtocolError, ProtocolErrorKind, ProtocolLimits,
    ProtocolVersion, ProviderExtension, ProviderName, RateLimitDimension, RateLimitObservation,
    RateLimitWindow, RedactedDiagnostic, ResetTime, ResponseId, Retryability, StreamFragment,
    ToolCallId, ToolName, TransportPhase, UsageCounters, UsageObservation, UsageScope,
};

/// Decodes canonical normalized-event schema-version-one bytes.
///
/// Decoding reconstructs inert normalized data only. Stream legality and terminal semantics are
/// revalidated when the returned envelope is replayed through [`crate::ResponseReducer`].
///
/// # Errors
///
/// Rejects unsupported versions, unknown tags, malformed nested values, noncanonical bytes,
/// trailing data, and values outside `limits`.
pub fn decode_event_envelope(
    bytes: &[u8],
    limits: ProtocolLimits,
) -> Result<EventEnvelope, ProtocolError> {
    if bytes.len() > MAX_CANONICAL_EVENT_BYTES {
        return Err(invalid("canonical_event", "canonical event exceeds its maximum byte bound"));
    }
    let mut reader = CanonicalReader::new(bytes, codec_limits(limits));
    if reader.read_fixed::<4>().map_err(read_codec)? != MAGIC {
        return Err(invalid("canonical_event.magic", "canonical event magic is invalid"));
    }
    let schema = reader.read_u16().map_err(read_codec)?;
    if schema != super::EVENT_ENVELOPE_SCHEMA_VERSION {
        return Err(ProtocolError::at(
            ProtocolErrorKind::UnsupportedVersion,
            "canonical_event.schema_version",
            "canonical event schema version is unsupported",
        ));
    }
    let protocol = ProtocolVersion::new(
        reader.read_u16().map_err(read_codec)?,
        reader.read_u16().map_err(read_codec)?,
    )?;
    if protocol != ProtocolVersion::V1 {
        return Err(ProtocolError::at(
            ProtocolErrorKind::UnsupportedVersion,
            "canonical_event.protocol_version",
            "canonical event protocol version is unsupported",
        ));
    }
    let sequence = reader.read_u64().map_err(read_codec)?;
    let provider_sequence = option_u64(&mut reader)?;
    let provider_event_id = if reader.read_option_tag().map_err(read_codec)? {
        Some(EventId::new(reader.read_str().map_err(read_codec)?.to_owned())?)
    } else {
        None
    };
    let provider_digest = Sha256Digest::new(reader.read_fixed::<32>().map_err(read_codec)?);
    let event = decode_event(&mut reader, limits)?;
    reader.finish().map_err(read_codec)?;
    let envelope =
        EventEnvelope::new(sequence, provider_sequence, provider_event_id, provider_digest, event)?;
    if super::encode::encode_event_envelope(&envelope, limits)?.as_slice() != bytes {
        return Err(invalid(
            "canonical_event",
            "decoded event bytes are not the canonical representation",
        ));
    }
    Ok(envelope)
}

#[allow(clippy::too_many_lines, reason = "closed event tags are decoded in one auditable table")]
fn decode_event(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ModelEvent, ProtocolError> {
    match reader.read_u8().map_err(read_codec)? {
        1 => Ok(ModelEvent::ResponseStarted {
            response_id: optional_response_id(reader)?,
            model: optional_model(reader)?,
        }),
        2 => ResponseId::new(reader.read_str().map_err(read_codec)?.to_owned())
            .map(ModelEvent::ResponseIdentity),
        3 => Ok(ModelEvent::ItemStarted {
            item_id: item_id(reader)?,
            index: reader.read_u32().map_err(read_codec)?,
            kind: decode_item_kind(reader.read_u8().map_err(read_codec)?)?,
        }),
        4 => Ok(ModelEvent::TextDelta {
            item_id: item_id(reader)?,
            fragment: fragment(reader, limits)?,
        }),
        5 => Ok(ModelEvent::ReasoningSummaryDelta {
            item_id: item_id(reader)?,
            fragment: fragment(reader, limits)?,
        }),
        6 => Ok(ModelEvent::ReasoningReplayDelta {
            item_id: item_id(reader)?,
            fragment: fragment(reader, limits)?,
        }),
        7 => Ok(ModelEvent::RefusalDelta {
            item_id: item_id(reader)?,
            fragment: fragment(reader, limits)?,
        }),
        8 => Ok(ModelEvent::ToolCallStarted {
            item_id: item_id(reader)?,
            call_id: tool_call_id(reader)?,
            name: ToolName::new(reader.read_str().map_err(read_codec)?.to_owned())?,
        }),
        9 => Ok(ModelEvent::ToolArgumentDelta {
            call_id: tool_call_id(reader)?,
            fragment: fragment(reader, limits)?,
        }),
        10 => item_id(reader).map(ModelEvent::ItemCompleted),
        11 => usage(reader, limits).map(ModelEvent::Usage),
        12 => rate_limits(reader).map(ModelEvent::RateLimit),
        13 => cache(reader).map(ModelEvent::Cache),
        14 => finish(reader, limits).map(ModelEvent::Finish),
        15 => provider_extension(reader, limits).map(ModelEvent::ProviderEvent),
        16 => Ok(ModelEvent::Heartbeat),
        17 => Ok(ModelEvent::ResponseCompleted),
        18 => failure(reader).map(ModelEvent::ResponseFailed),
        19 => Ok(ModelEvent::ResponseCancelled),
        _ => Err(unknown("model_event")),
    }
}

fn optional_response_id(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<ResponseId>, ProtocolError> {
    if reader.read_option_tag().map_err(read_codec)? {
        ResponseId::new(reader.read_str().map_err(read_codec)?.to_owned()).map(Some)
    } else {
        Ok(None)
    }
}

fn optional_model(reader: &mut CanonicalReader<'_>) -> Result<Option<ModelName>, ProtocolError> {
    if reader.read_option_tag().map_err(read_codec)? {
        ModelName::new(reader.read_str().map_err(read_codec)?.to_owned()).map(Some)
    } else {
        Ok(None)
    }
}

fn item_id(reader: &mut CanonicalReader<'_>) -> Result<ItemId, ProtocolError> {
    ItemId::new(reader.read_str().map_err(read_codec)?.to_owned())
}

fn tool_call_id(reader: &mut CanonicalReader<'_>) -> Result<ToolCallId, ProtocolError> {
    ToolCallId::new(reader.read_str().map_err(read_codec)?.to_owned())
}

fn fragment(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<StreamFragment, ProtocolError> {
    StreamFragment::new(reader.read_bytes_owned().map_err(read_codec)?, limits)
}

fn decode_item_kind(tag: u8) -> Result<ItemKind, ProtocolError> {
    match tag {
        1 => Ok(ItemKind::Message),
        2 => Ok(ItemKind::StructuredOutput),
        3 => Ok(ItemKind::ToolCall),
        4 => Ok(ItemKind::Reasoning),
        5 => Ok(ItemKind::Refusal),
        6 => Ok(ItemKind::ProviderNative),
        _ => Err(unknown("item_kind")),
    }
}

fn usage(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<UsageObservation, ProtocolError> {
    let scope = match reader.read_u8().map_err(read_codec)? {
        1 => UsageScope::Step,
        2 => UsageScope::Cumulative,
        3 => UsageScope::Final,
        _ => return Err(unknown("usage_scope")),
    };
    let counters = UsageCounters::new(
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
    );
    let detail = if reader.read_option_tag().map_err(read_codec)? {
        Some(canonical_json(reader, limits)?)
    } else {
        None
    };
    Ok(UsageObservation::new(scope, counters, detail))
}

fn canonical_json(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<CanonicalJson, ProtocolError> {
    let bytes = reader.read_bytes().map_err(read_codec)?;
    let text = core::str::from_utf8(bytes)
        .map_err(|_| invalid("canonical_event.json", "canonical JSON is not UTF-8"))?;
    let value = CanonicalJson::parse(text, JsonBounds::value(limits))?;
    if value.canonical_bytes() != bytes {
        return Err(invalid("canonical_event.json", "JSON bytes are not canonical"));
    }
    Ok(value)
}

fn rate_limits(reader: &mut CanonicalReader<'_>) -> Result<RateLimitObservation, ProtocolError> {
    let count = reader.read_collection_len().map_err(read_codec)?;
    if count == 0 || count > 64 {
        return Err(invalid(
            "canonical_event.rate_limits",
            "rate-limit window count is outside its bound",
        ));
    }
    let mut windows = Vec::with_capacity(count);
    for _ in 0..count {
        let dimension = match reader.read_u8().map_err(read_codec)? {
            1 => RateLimitDimension::Requests,
            2 => RateLimitDimension::InputTokens,
            3 => RateLimitDimension::OutputTokens,
            4 => RateLimitDimension::TotalTokens,
            5 => RateLimitDimension::Images,
            6 => RateLimitDimension::DailyRequests,
            7 => RateLimitDimension::Provider(ExtensionName::new(
                reader.read_str().map_err(read_codec)?.to_owned(),
            )?),
            _ => return Err(unknown("rate_limit_dimension")),
        };
        let limit = option_u64(reader)?;
        let remaining = option_u64(reader)?;
        let reset = if reader.read_option_tag().map_err(read_codec)? {
            let kind = reader.read_u8().map_err(read_codec)?;
            let value = reader.read_u64().map_err(read_codec)?;
            Some(match kind {
                1 => ResetTime::AfterMillis(value),
                2 => ResetTime::UnixMillis(value),
                _ => return Err(unknown("rate_limit_reset")),
            })
        } else {
            None
        };
        windows.push(RateLimitWindow::new(dimension, limit, remaining, reset)?);
    }
    RateLimitObservation::new(windows)
}

fn cache(reader: &mut CanonicalReader<'_>) -> Result<CacheObservation, ProtocolError> {
    let status = match reader.read_u8().map_err(read_codec)? {
        1 => CacheStatus::Hit,
        2 => CacheStatus::Miss,
        3 => CacheStatus::Created,
        4 => CacheStatus::Bypassed,
        5 => CacheStatus::Unknown,
        _ => return Err(unknown("cache_status")),
    };
    let key = if reader.read_option_tag().map_err(read_codec)? {
        Some(CacheKey::new(reader.read_str().map_err(read_codec)?.to_owned())?)
    } else {
        None
    };
    Ok(CacheObservation::new(status, key, option_u64(reader)?, option_u64(reader)?))
}

fn finish(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<FinishReason, ProtocolError> {
    match reader.read_u8().map_err(read_codec)? {
        1 => Ok(FinishReason::Stop),
        2 => Ok(FinishReason::Length),
        3 => Ok(FinishReason::ToolCalls),
        4 => Ok(FinishReason::Safety),
        5 => Ok(FinishReason::Refusal),
        6 => Ok(FinishReason::Pause),
        7 => Ok(FinishReason::ContextLimit),
        8 => Ok(FinishReason::Cancelled),
        9 => Ok(FinishReason::Incomplete),
        10 => BoundedText::new(reader.read_str().map_err(read_codec)?.to_owned(), limits)
            .map(FinishReason::Provider),
        _ => Err(unknown("finish_reason")),
    }
}

fn provider_extension(
    reader: &mut CanonicalReader<'_>,
    limits: ProtocolLimits,
) -> Result<ProviderExtension, ProtocolError> {
    let name = ExtensionName::new(reader.read_str().map_err(read_codec)?.to_owned())?;
    Ok(ProviderExtension::new(name, canonical_json(reader, limits)?))
}

fn failure(reader: &mut CanonicalReader<'_>) -> Result<ModelFailure, ProtocolError> {
    let provider = ProviderName::new(reader.read_str().map_err(read_codec)?.to_owned())?;
    let category = failure_category(reader.read_u8().map_err(read_codec)?)?;
    let phase = transport_phase(reader.read_u8().map_err(read_codec)?)?;
    let certainty = certainty(reader.read_u8().map_err(read_codec)?)?;
    let retryability = retryability(reader.read_u8().map_err(read_codec)?)?;
    let status = option_u16(reader)?;
    let response_id = optional_response_id(reader)?;
    let retry_after = option_u64(reader)?;
    let diagnostic = RedactedDiagnostic::new(
        reader.read_str().map_err(read_codec)?.to_owned(),
        option_u64(reader)?,
        option_u64(reader)?,
        option_u64(reader)?,
    )?;
    Ok(ModelFailure::new(
        provider,
        category,
        phase,
        certainty,
        retryability,
        status,
        response_id,
        retry_after,
        diagnostic,
    ))
}

fn failure_category(tag: u8) -> Result<FailureCategory, ProtocolError> {
    match tag {
        1 => Ok(FailureCategory::InvalidRequest),
        2 => Ok(FailureCategory::Authentication),
        3 => Ok(FailureCategory::Permission),
        4 => Ok(FailureCategory::NotFound),
        5 => Ok(FailureCategory::RateLimited),
        6 => Ok(FailureCategory::QuotaExhausted),
        7 => Ok(FailureCategory::TransientProvider),
        8 => Ok(FailureCategory::Transport),
        9 => Ok(FailureCategory::AmbiguousAcceptance),
        10 => Ok(FailureCategory::MalformedPayload),
        11 => Ok(FailureCategory::IncompleteStream),
        12 => Ok(FailureCategory::Timeout),
        13 => Ok(FailureCategory::Refusal),
        14 => Ok(FailureCategory::Safety),
        15 => Ok(FailureCategory::Cancellation),
        16 => Ok(FailureCategory::Provider),
        _ => Err(unknown("failure_category")),
    }
}

fn transport_phase(tag: u8) -> Result<TransportPhase, ProtocolError> {
    match tag {
        1 => Ok(TransportPhase::BeforeSend),
        2 => Ok(TransportPhase::Connecting),
        3 => Ok(TransportPhase::SendingHeaders),
        4 => Ok(TransportPhase::SendingBody),
        5 => Ok(TransportPhase::AwaitingHeaders),
        6 => Ok(TransportPhase::ReadingBody),
        7 => Ok(TransportPhase::StreamObserved),
        8 => Ok(TransportPhase::Completed),
        _ => Err(unknown("transport_phase")),
    }
}

fn certainty(tag: u8) -> Result<OutcomeCertainty, ProtocolError> {
    match tag {
        1 => Ok(OutcomeCertainty::DefinitelyNotAccepted),
        2 => Ok(OutcomeCertainty::MaybeAccepted),
        3 => Ok(OutcomeCertainty::AcceptedPartial),
        4 => Ok(OutcomeCertainty::Terminal),
        _ => Err(unknown("outcome_certainty")),
    }
}

fn retryability(tag: u8) -> Result<Retryability, ProtocolError> {
    match tag {
        1 => Ok(Retryability::Never),
        2 => Ok(Retryability::SafeNewRequest),
        3 => Ok(Retryability::ExactResumeOnly),
        4 => Ok(Retryability::CallerDecision),
        _ => Err(unknown("retryability")),
    }
}
