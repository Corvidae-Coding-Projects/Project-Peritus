use peritus_codec::CanonicalWriter;

use super::primitive::{MAGIC, codec_limits, write_codec, write_option_u16, write_option_u64};
use crate::{
    CacheObservation, CacheStatus, EventEnvelope, FailureCategory, FinishReason, ItemKind,
    ModelEvent, ModelFailure, OutcomeCertainty, RateLimitDimension, RateLimitObservation,
    ResetTime, Retryability, TransportPhase, UsageCounters, UsageObservation, UsageScope,
};

/// Encodes one normalized event envelope into canonical schema-version-one bytes.
///
/// Provider raw bytes are not included; the envelope's exact provider digest and all normalized
/// semantics are retained. Encoding does not validate stream ordering, which remains the
/// responsibility of [`crate::ResponseReducer`].
///
/// # Errors
///
/// Returns a typed limit error if a checked value cannot fit the canonical event ceiling.
pub fn encode_event_envelope(
    envelope: &EventEnvelope,
    limits: crate::ProtocolLimits,
) -> Result<Vec<u8>, crate::ProtocolError> {
    let mut writer = CanonicalWriter::new(codec_limits(limits));
    writer.write_fixed(&MAGIC).map_err(write_codec)?;
    writer.write_u16(super::EVENT_ENVELOPE_SCHEMA_VERSION).map_err(write_codec)?;
    writer.write_u16(envelope.protocol().major()).map_err(write_codec)?;
    writer.write_u16(envelope.protocol().minor()).map_err(write_codec)?;
    writer.write_u64(envelope.sequence()).map_err(write_codec)?;
    write_option_u64(&mut writer, envelope.provider_sequence())?;
    writer.write_option_tag(envelope.provider_event_id().is_some()).map_err(write_codec)?;
    if let Some(id) = envelope.provider_event_id() {
        writer.write_str(id.expose_for_wire()).map_err(write_codec)?;
    }
    writer.write_fixed(envelope.provider_digest().as_bytes()).map_err(write_codec)?;
    event(&mut writer, envelope.event())?;
    Ok(writer.into_bytes())
}

fn event(writer: &mut CanonicalWriter, event: &ModelEvent) -> Result<(), crate::ProtocolError> {
    match event {
        ModelEvent::ResponseStarted { response_id, model } => {
            writer.write_u8(1).map_err(write_codec)?;
            writer.write_option_tag(response_id.is_some()).map_err(write_codec)?;
            if let Some(id) = response_id {
                writer.write_str(id.expose_for_wire()).map_err(write_codec)?;
            }
            writer.write_option_tag(model.is_some()).map_err(write_codec)?;
            if let Some(model) = model {
                writer.write_str(model.as_str()).map_err(write_codec)?;
            }
        }
        ModelEvent::ResponseIdentity(id) => {
            writer.write_u8(2).map_err(write_codec)?;
            writer.write_str(id.expose_for_wire()).map_err(write_codec)?;
        }
        ModelEvent::ItemStarted { item_id, index, kind } => {
            writer.write_u8(3).map_err(write_codec)?;
            writer.write_str(item_id.expose_for_wire()).map_err(write_codec)?;
            writer.write_u32(*index).map_err(write_codec)?;
            writer.write_u8(item_kind(*kind)).map_err(write_codec)?;
        }
        ModelEvent::TextDelta { item_id, fragment } => {
            fragment_event(writer, 4, item_id.expose_for_wire(), fragment.expose())?;
        }
        ModelEvent::ReasoningSummaryDelta { item_id, fragment } => {
            fragment_event(writer, 5, item_id.expose_for_wire(), fragment.expose())?;
        }
        ModelEvent::ReasoningReplayDelta { item_id, fragment } => {
            fragment_event(writer, 6, item_id.expose_for_wire(), fragment.expose())?;
        }
        ModelEvent::RefusalDelta { item_id, fragment } => {
            fragment_event(writer, 7, item_id.expose_for_wire(), fragment.expose())?;
        }
        ModelEvent::ToolCallStarted { item_id, call_id, name } => {
            writer.write_u8(8).map_err(write_codec)?;
            writer.write_str(item_id.expose_for_wire()).map_err(write_codec)?;
            writer.write_str(call_id.expose_for_wire()).map_err(write_codec)?;
            writer.write_str(name.as_str()).map_err(write_codec)?;
        }
        ModelEvent::ToolArgumentDelta { call_id, fragment } => {
            fragment_event(writer, 9, call_id.expose_for_wire(), fragment.expose())?;
        }
        ModelEvent::ItemCompleted(id) => {
            writer.write_u8(10).map_err(write_codec)?;
            writer.write_str(id.expose_for_wire()).map_err(write_codec)?;
        }
        ModelEvent::Usage(observation) => {
            writer.write_u8(11).map_err(write_codec)?;
            usage(writer, observation)?;
        }
        ModelEvent::RateLimit(observation) => {
            writer.write_u8(12).map_err(write_codec)?;
            rate_limits(writer, observation)?;
        }
        ModelEvent::Cache(observation) => {
            writer.write_u8(13).map_err(write_codec)?;
            cache(writer, observation)?;
        }
        ModelEvent::Finish(reason) => {
            writer.write_u8(14).map_err(write_codec)?;
            finish(writer, reason)?;
        }
        ModelEvent::ProviderEvent(extension) => {
            writer.write_u8(15).map_err(write_codec)?;
            writer.write_str(extension.name().as_str()).map_err(write_codec)?;
            writer.write_bytes(extension.value().canonical_bytes()).map_err(write_codec)?;
        }
        ModelEvent::Heartbeat => writer.write_u8(16).map_err(write_codec)?,
        ModelEvent::ResponseCompleted => writer.write_u8(17).map_err(write_codec)?,
        ModelEvent::ResponseFailed(failure) => {
            writer.write_u8(18).map_err(write_codec)?;
            failure_value(writer, failure)?;
        }
        ModelEvent::ResponseCancelled => writer.write_u8(19).map_err(write_codec)?,
    }
    Ok(())
}

fn fragment_event(
    writer: &mut CanonicalWriter,
    tag: u8,
    identity: &str,
    bytes: &[u8],
) -> Result<(), crate::ProtocolError> {
    writer.write_u8(tag).map_err(write_codec)?;
    writer.write_str(identity).map_err(write_codec)?;
    writer.write_bytes(bytes).map_err(write_codec)
}

const fn item_kind(kind: ItemKind) -> u8 {
    match kind {
        ItemKind::Message => 1,
        ItemKind::StructuredOutput => 2,
        ItemKind::ToolCall => 3,
        ItemKind::Reasoning => 4,
        ItemKind::Refusal => 5,
        ItemKind::ProviderNative => 6,
    }
}

fn usage(
    writer: &mut CanonicalWriter,
    observation: &UsageObservation,
) -> Result<(), crate::ProtocolError> {
    writer
        .write_u8(match observation.scope() {
            UsageScope::Step => 1,
            UsageScope::Cumulative => 2,
            UsageScope::Final => 3,
        })
        .map_err(write_codec)?;
    usage_counters(writer, observation.counters())?;
    writer.write_option_tag(observation.provider_detail().is_some()).map_err(write_codec)?;
    if let Some(detail) = observation.provider_detail() {
        writer.write_bytes(detail.canonical_bytes()).map_err(write_codec)?;
    }
    Ok(())
}

fn usage_counters(
    writer: &mut CanonicalWriter,
    counters: UsageCounters,
) -> Result<(), crate::ProtocolError> {
    for value in [
        counters.input_tokens(),
        counters.cached_input_tokens(),
        counters.cache_creation_input_tokens(),
        counters.output_tokens(),
        counters.reasoning_output_tokens(),
        counters.tool_tokens(),
        counters.total_tokens(),
        counters.provider_cost_microunits(),
    ] {
        write_option_u64(writer, value)?;
    }
    Ok(())
}

fn rate_limits(
    writer: &mut CanonicalWriter,
    observation: &RateLimitObservation,
) -> Result<(), crate::ProtocolError> {
    writer.write_collection_len(observation.windows().len()).map_err(write_codec)?;
    for window in observation.windows() {
        match window.dimension() {
            RateLimitDimension::Requests => writer.write_u8(1).map_err(write_codec)?,
            RateLimitDimension::InputTokens => writer.write_u8(2).map_err(write_codec)?,
            RateLimitDimension::OutputTokens => writer.write_u8(3).map_err(write_codec)?,
            RateLimitDimension::TotalTokens => writer.write_u8(4).map_err(write_codec)?,
            RateLimitDimension::Images => writer.write_u8(5).map_err(write_codec)?,
            RateLimitDimension::DailyRequests => writer.write_u8(6).map_err(write_codec)?,
            RateLimitDimension::Provider(name) => {
                writer.write_u8(7).map_err(write_codec)?;
                writer.write_str(name.as_str()).map_err(write_codec)?;
            }
        }
        write_option_u64(writer, window.limit())?;
        write_option_u64(writer, window.remaining())?;
        writer.write_option_tag(window.reset().is_some()).map_err(write_codec)?;
        if let Some(reset) = window.reset() {
            match reset {
                ResetTime::AfterMillis(value) => {
                    writer.write_u8(1).map_err(write_codec)?;
                    writer.write_u64(value).map_err(write_codec)?;
                }
                ResetTime::UnixMillis(value) => {
                    writer.write_u8(2).map_err(write_codec)?;
                    writer.write_u64(value).map_err(write_codec)?;
                }
            }
        }
    }
    Ok(())
}

fn cache(
    writer: &mut CanonicalWriter,
    observation: &CacheObservation,
) -> Result<(), crate::ProtocolError> {
    writer
        .write_u8(match observation.status() {
            CacheStatus::Hit => 1,
            CacheStatus::Miss => 2,
            CacheStatus::Created => 3,
            CacheStatus::Bypassed => 4,
            CacheStatus::Unknown => 5,
        })
        .map_err(write_codec)?;
    writer.write_option_tag(observation.key().is_some()).map_err(write_codec)?;
    if let Some(key) = observation.key() {
        writer.write_str(key.expose_for_wire()).map_err(write_codec)?;
    }
    write_option_u64(writer, observation.input_tokens())?;
    write_option_u64(writer, observation.ttl_seconds())
}

fn finish(writer: &mut CanonicalWriter, reason: &FinishReason) -> Result<(), crate::ProtocolError> {
    match reason {
        FinishReason::Stop => writer.write_u8(1).map_err(write_codec)?,
        FinishReason::Length => writer.write_u8(2).map_err(write_codec)?,
        FinishReason::ToolCalls => writer.write_u8(3).map_err(write_codec)?,
        FinishReason::Safety => writer.write_u8(4).map_err(write_codec)?,
        FinishReason::Refusal => writer.write_u8(5).map_err(write_codec)?,
        FinishReason::Pause => writer.write_u8(6).map_err(write_codec)?,
        FinishReason::ContextLimit => writer.write_u8(7).map_err(write_codec)?,
        FinishReason::Cancelled => writer.write_u8(8).map_err(write_codec)?,
        FinishReason::Incomplete => writer.write_u8(9).map_err(write_codec)?,
        FinishReason::Provider(value) => {
            writer.write_u8(10).map_err(write_codec)?;
            writer.write_str(value.expose_for_wire()).map_err(write_codec)?;
        }
    }
    Ok(())
}

fn failure_value(
    writer: &mut CanonicalWriter,
    failure: &ModelFailure,
) -> Result<(), crate::ProtocolError> {
    writer.write_str(failure.provider().as_str()).map_err(write_codec)?;
    writer.write_u8(failure_category(failure.category())).map_err(write_codec)?;
    writer.write_u8(transport_phase(failure.phase())).map_err(write_codec)?;
    writer.write_u8(certainty(failure.certainty())).map_err(write_codec)?;
    writer.write_u8(retryability(failure.retryability())).map_err(write_codec)?;
    write_option_u16(writer, failure.http_status())?;
    writer.write_option_tag(failure.response_id().is_some()).map_err(write_codec)?;
    if let Some(id) = failure.response_id() {
        writer.write_str(id.expose_for_wire()).map_err(write_codec)?;
    }
    write_option_u64(writer, failure.retry_after_millis())?;
    let diagnostic = failure.diagnostic();
    writer.write_str(diagnostic.code()).map_err(write_codec)?;
    write_option_u64(writer, diagnostic.request_bytes())?;
    write_option_u64(writer, diagnostic.response_bytes())?;
    write_option_u64(writer, diagnostic.elapsed_millis())
}

const fn failure_category(value: FailureCategory) -> u8 {
    match value {
        FailureCategory::InvalidRequest => 1,
        FailureCategory::Authentication => 2,
        FailureCategory::Permission => 3,
        FailureCategory::NotFound => 4,
        FailureCategory::RateLimited => 5,
        FailureCategory::QuotaExhausted => 6,
        FailureCategory::TransientProvider => 7,
        FailureCategory::Transport => 8,
        FailureCategory::AmbiguousAcceptance => 9,
        FailureCategory::MalformedPayload => 10,
        FailureCategory::IncompleteStream => 11,
        FailureCategory::Timeout => 12,
        FailureCategory::Refusal => 13,
        FailureCategory::Safety => 14,
        FailureCategory::Cancellation => 15,
        FailureCategory::Provider => 16,
    }
}

const fn transport_phase(value: TransportPhase) -> u8 {
    match value {
        TransportPhase::BeforeSend => 1,
        TransportPhase::Connecting => 2,
        TransportPhase::SendingHeaders => 3,
        TransportPhase::SendingBody => 4,
        TransportPhase::AwaitingHeaders => 5,
        TransportPhase::ReadingBody => 6,
        TransportPhase::StreamObserved => 7,
        TransportPhase::Completed => 8,
    }
}

const fn certainty(value: OutcomeCertainty) -> u8 {
    match value {
        OutcomeCertainty::DefinitelyNotAccepted => 1,
        OutcomeCertainty::MaybeAccepted => 2,
        OutcomeCertainty::AcceptedPartial => 3,
        OutcomeCertainty::Terminal => 4,
    }
}

const fn retryability(value: Retryability) -> u8 {
    match value {
        Retryability::Never => 1,
        Retryability::SafeNewRequest => 2,
        Retryability::ExactResumeOnly => 3,
        Retryability::CallerDecision => 4,
    }
}
