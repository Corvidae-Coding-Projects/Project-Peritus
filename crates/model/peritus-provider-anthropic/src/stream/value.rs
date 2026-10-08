//! Bounded JSON extraction and normalized metadata helpers.

use std::io;

use peritus_model_protocol::{
    BoundedText, CacheObservation, CacheStatus, CanonicalJson, ExtensionName, FinishReason, ItemId,
    JsonBounds, ModelEvent, ProtocolLimits, ProviderExtension, RateLimitDimension,
    RateLimitObservation, RateLimitWindow, UsageCounters, UsageObservation, UsageScope,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError};
use serde::Serialize;
use serde_json::{Map, Value};

use super::state::UsageState;

pub(super) const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::malformed_stream("anthropic_stream", detail)
}

pub(super) const fn limit(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::limit_exceeded("anthropic_stream", detail)
}

pub(super) fn owned_string(
    value: &str,
    detail: &'static str,
) -> Result<String, ProviderCoreError> {
    let mut owned = String::new();
    owned.try_reserve_exact(value.len()).map_err(|_| limit(detail))?;
    owned.push_str(value);
    Ok(owned)
}

pub(super) fn required_str<'a>(
    value: &'a Value,
    pointer: &str,
) -> Result<&'a str, ProviderCoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Anthropic event string field is missing or invalid"))
}

pub(super) fn required_u32(value: &Value, pointer: &str) -> Result<u32, ProviderCoreError> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| invalid("Anthropic event index is missing or out of range"))
}

pub(super) fn item_id(
    response_id: &peritus_model_protocol::ResponseId,
    index: u32,
) -> Result<ItemId, ProviderCoreError> {
    use core::fmt::Write as _;
    use sha2::{Digest as _, Sha256};

    const MAX_NORMALIZED_ID_BYTES: usize = 512;
    const DERIVED_PREFIX: &str = "anthropic-item-v1-";

    let base = response_id.expose_for_wire();
    let mut suffix = String::new();
    suffix
        .try_reserve_exact(11)
        .map_err(|_| limit("Anthropic item identity capacity is unavailable"))?;
    write!(&mut suffix, ":{index}")
        .map_err(|_| invalid("Anthropic item identity formatting failed"))?;
    if base
        .len()
        .checked_add(suffix.len())
        .is_some_and(|length| length <= MAX_NORMALIZED_ID_BYTES)
    {
        let capacity = base
            .len()
            .checked_add(suffix.len())
            .ok_or_else(|| limit("Anthropic item identity length overflowed"))?;
        let mut value = String::new();
        value
            .try_reserve_exact(capacity)
            .map_err(|_| limit("Anthropic item identity capacity is unavailable"))?;
        value.push_str(base);
        value.push_str(&suffix);
        return ItemId::new(value)
            .map_err(|_| invalid("Anthropic-derived item identity is invalid"));
    }
    let base_length = u64::try_from(base.len())
        .map_err(|_| limit("Anthropic response identity length exceeded u64"))?;
    let suffix_length = u64::try_from(suffix.len())
        .map_err(|_| limit("Anthropic item identity suffix length exceeded u64"))?;
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.anthropic.item.v1\0");
    hasher.update(base_length.to_be_bytes());
    hasher.update(base.as_bytes());
    hasher.update(suffix_length.to_be_bytes());
    hasher.update(suffix.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut value = String::new();
    value
        .try_reserve_exact(DERIVED_PREFIX.len() + digest.len() * 2)
        .map_err(|_| limit("Anthropic derived identity capacity is unavailable"))?;
    value.push_str(DERIVED_PREFIX);
    for byte in digest {
        write!(&mut value, "{byte:02x}")
            .map_err(|_| invalid("Anthropic derived identity formatting failed"))?;
    }
    ItemId::new(value).map_err(|_| invalid("Anthropic-derived item identity is invalid"))
}

pub(super) fn usage_event(usage: &UsageState, scope: UsageScope) -> ModelEvent {
    let total = usage.input.zip(usage.output).and_then(|(input, output)| input.checked_add(output));
    ModelEvent::Usage(UsageObservation::new(
        scope,
        UsageCounters::new(
            usage.input,
            usage.cache_read,
            usage.cache_creation,
            usage.output,
            None,
            None,
            total,
            None,
        ),
        None,
    ))
}

pub(super) fn cache_events(usage: &UsageState) -> Vec<ModelEvent> {
    let mut events = Vec::new();
    if usage.cache_creation.unwrap_or(0) > 0 {
        events.push(ModelEvent::Cache(CacheObservation::new(
            CacheStatus::Created,
            None,
            usage.cache_creation,
            None,
        )));
    }
    if usage.cache_read.unwrap_or(0) > 0 {
        events.push(ModelEvent::Cache(CacheObservation::new(
            CacheStatus::Hit,
            None,
            usage.cache_read,
            None,
        )));
    }
    events
}

pub(super) fn finish_reason(
    raw: &str,
    limits: ProtocolLimits,
) -> Result<FinishReason, ProviderCoreError> {
    Ok(match raw {
        "end_turn" | "stop_sequence" => FinishReason::Stop,
        "max_tokens" => FinishReason::Length,
        "tool_use" => FinishReason::ToolCalls,
        "pause_turn" => FinishReason::Pause,
        "refusal" => FinishReason::Refusal,
        "model_context_window_exceeded" => FinishReason::ContextLimit,
        unknown => FinishReason::Provider(
            BoundedText::new(owned_string(
                unknown,
                "Anthropic stop reason capacity is unavailable",
            )?, limits)
                .map_err(|_| invalid("Anthropic stop reason exceeds protocol bounds"))?,
        ),
    })
}

pub(super) fn provider_event(
    name: &str,
    value: &Value,
    limits: ProtocolLimits,
) -> Result<ModelEvent, ProviderCoreError> {
    let name = ExtensionName::new(name.to_owned())
        .map_err(|_| invalid("Anthropic provider event name is invalid"))?;
    let bytes = serde_json::to_string(value)
        .map_err(|_| invalid("Anthropic provider event serialization failed"))?;
    let value = CanonicalJson::parse(&bytes, JsonBounds::value(limits))
        .map_err(|_| invalid("Anthropic provider event exceeds JSON bounds"))?;
    Ok(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)))
}

#[derive(Clone, Copy)]
pub(super) enum ReplayKind {
    ThinkingSignature,
    RedactedThinking,
}

impl ReplayKind {
    const fn syntax(self) -> (&'static [u8], &'static [u8]) {
        match self {
            Self::ThinkingSignature => {
                (b"{\"signature\":\"", b"\",\"type\":\"thinking\"}")
            }
            Self::RedactedThinking => {
                (b"{\"data\":\"", b"\",\"type\":\"redacted_thinking\"}")
            }
        }
    }
}

pub(super) struct ReplayBytes {
    tasks: Vec<ReplayTask>,
    expected: usize,
    emitted: usize,
}

impl ReplayBytes {
    pub(super) fn new(
        kind: ReplayKind,
        value: &str,
        maximum: usize,
    ) -> Result<Self, ProviderCoreError> {
        let (prefix, suffix) = kind.syntax();
        let string_bytes = encoded_len(value)?
            .checked_sub(2)
            .ok_or_else(|| invalid("Anthropic replay string encoding was invalid"))?;
        let expected = checked_sum(&[prefix.len(), string_bytes, suffix.len()])?;
        if expected > maximum {
            return Err(limit("Anthropic reasoning replay exceeded its aggregate bound"));
        }
        let value = owned_string(value, "Anthropic reasoning replay capacity is unavailable")?;
        let mut tasks = Vec::new();
        tasks
            .try_reserve_exact(if value.is_empty() { 2 } else { 3 })
            .map_err(|_| limit("Anthropic reasoning replay capacity is unavailable"))?;
        tasks.push(ReplayTask::Static { bytes: suffix, offset: 0 });
        if !value.is_empty() {
            tasks.push(ReplayTask::StringContent { value, offset: 0 });
        }
        tasks.push(ReplayTask::Static { bytes: prefix, offset: 0 });
        Ok(Self { tasks, expected, emitted: 0 })
    }

    pub(super) fn fragment_count(&self, maximum: usize) -> Result<usize, ProviderCoreError> {
        if maximum == 0 {
            return Err(limit("Anthropic reasoning replay fragment bound was zero"));
        }
        self.expected
            .checked_add(maximum - 1)
            .map(|value| value / maximum)
            .ok_or_else(|| limit("Anthropic reasoning replay fragment count overflowed"))
    }

    pub(super) fn next_chunk(
        &mut self,
        maximum: usize,
    ) -> Result<Option<Vec<u8>>, ProviderCoreError> {
        if maximum == 0 {
            return Err(limit("Anthropic reasoning replay fragment bound was zero"));
        }
        let remaining = self
            .expected
            .checked_sub(self.emitted)
            .ok_or_else(|| invalid("Anthropic reasoning replay size accounting changed"))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(remaining.min(maximum))
            .map_err(|_| limit("Anthropic reasoning replay capacity is unavailable"))?;
        while output.len() < maximum {
            let Some(task) = self.tasks.pop() else { break };
            match task {
                ReplayTask::Static { bytes, offset } => {
                    let next = copy_piece(&mut output, maximum, bytes, offset, bytes.len());
                    if next < bytes.len() {
                        self.tasks.push(ReplayTask::Static { bytes, offset: next });
                    }
                }
                ReplayTask::Owned { bytes, offset, end } => {
                    let next = copy_piece(&mut output, maximum, &bytes, offset, end);
                    if next < end {
                        self.tasks.push(ReplayTask::Owned { bytes, offset: next, end });
                    }
                }
                ReplayTask::StringContent { value, offset } => {
                    self.expand_string(value, offset, maximum - output.len())?;
                }
            }
        }
        let next = self
            .emitted
            .checked_add(output.len())
            .ok_or_else(|| limit("Anthropic reasoning replay size overflowed"))?;
        if next > self.expected {
            return Err(invalid("Anthropic reasoning replay exceeded its canonical size"));
        }
        self.emitted = next;
        if output.is_empty() {
            if self.emitted != self.expected {
                return Err(invalid("Anthropic reasoning replay canonical size changed"));
            }
            return Ok(None);
        }
        Ok(Some(output))
    }

    fn expand_string(
        &mut self,
        value: String,
        offset: usize,
        available: usize,
    ) -> Result<(), ProviderCoreError> {
        if offset == value.len() {
            return Ok(());
        }
        let source_bytes = available.max(1).min(2_048);
        let mut end = offset.saturating_add(source_bytes).min(value.len());
        while end > offset && !value.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset {
            let width = value[offset..]
                .chars()
                .next()
                .ok_or_else(|| invalid("Anthropic reasoning replay cursor was invalid"))?
                .len_utf8();
            end = offset
                .checked_add(width)
                .ok_or_else(|| limit("Anthropic reasoning replay cursor overflowed"))?;
        }
        let source_len = end - offset;
        let capacity = source_len
            .checked_mul(6)
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| limit("Anthropic reasoning replay encoding size overflowed"))?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(capacity)
            .map_err(|_| limit("Anthropic reasoning replay capacity is unavailable"))?;
        serde_json::to_writer(&mut encoded, &value[offset..end])
            .map_err(|_| invalid("Anthropic reasoning replay serialization failed"))?;
        if encoded.len() < 2 {
            return Err(invalid("Anthropic reasoning replay string encoding was invalid"));
        }
        if end < value.len() {
            self.tasks.push(ReplayTask::StringContent { value, offset: end });
        }
        let encoded_end = encoded.len() - 1;
        self.tasks.push(ReplayTask::Owned { bytes: encoded, offset: 1, end: encoded_end });
        Ok(())
    }
}

enum ReplayTask {
    Static { bytes: &'static [u8], offset: usize },
    Owned { bytes: Vec<u8>, offset: usize, end: usize },
    StringContent { value: String, offset: usize },
}

fn copy_piece(
    output: &mut Vec<u8>,
    maximum: usize,
    source: &[u8],
    offset: usize,
    end: usize,
) -> usize {
    let amount = (maximum - output.len()).min(end - offset);
    let next = offset + amount;
    output.extend_from_slice(&source[offset..next]);
    next
}

fn encoded_len<T: ?Sized + Serialize>(value: &T) -> Result<usize, ProviderCoreError> {
    let mut writer = CountingWriter { length: 0, overflowed: false };
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(if writer.overflowed {
            limit("Anthropic reasoning replay size overflowed")
        } else {
            invalid("Anthropic reasoning replay serialization failed")
        });
    }
    Ok(writer.length)
}

fn checked_sum(values: &[usize]) -> Result<usize, ProviderCoreError> {
    values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| limit("Anthropic reasoning replay size overflowed"))
    })
}

struct CountingWriter {
    length: usize,
    overflowed: bool,
}

impl io::Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(next) = self.length.checked_add(buffer.len()) else {
            self.overflowed = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "Anthropic replay size overflowed",
            ));
        };
        self.length = next;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn metadata_events(
    headers: &HttpHeaders,
    limits: ProtocolLimits,
) -> Result<Vec<ModelEvent>, ProviderCoreError> {
    let mut events = Vec::new();
    if let Some(request_id) =
        header_text(headers, "request-id").or_else(|| header_text(headers, "x-request-id"))
    {
        let mut metadata = Map::new();
        metadata.insert("request_id".to_owned(), Value::String(request_id.to_owned()));
        events.push(provider_event(
            "anthropic.request_metadata",
            &Value::Object(metadata),
            limits,
        )?);
    }
    let dimensions = [
        (
            RateLimitDimension::Requests,
            "anthropic-ratelimit-requests-limit",
            "anthropic-ratelimit-requests-remaining",
        ),
        (
            RateLimitDimension::InputTokens,
            "anthropic-ratelimit-input-tokens-limit",
            "anthropic-ratelimit-input-tokens-remaining",
        ),
        (
            RateLimitDimension::OutputTokens,
            "anthropic-ratelimit-output-tokens-limit",
            "anthropic-ratelimit-output-tokens-remaining",
        ),
    ];
    let mut windows = Vec::new();
    for (dimension, limit_name, remaining_name) in dimensions {
        let limit = header_u64(headers, limit_name);
        let remaining = header_u64(headers, remaining_name);
        if limit.is_some() || remaining.is_some() {
            windows.push(
                RateLimitWindow::new(dimension, limit, remaining, None)
                    .map_err(|_| invalid("Anthropic rate-limit headers are inconsistent"))?,
            );
        }
    }
    if !windows.is_empty() {
        events.push(ModelEvent::RateLimit(
            RateLimitObservation::new(windows)
                .map_err(|_| invalid("Anthropic rate-limit metadata exceeds bounds"))?,
        ));
    }
    Ok(events)
}

fn header_u64(headers: &HttpHeaders, name: &str) -> Option<u64> {
    header_text(headers, name)?.parse().ok()
}

fn header_text<'a>(headers: &'a HttpHeaders, name: &str) -> Option<&'a str> {
    core::str::from_utf8(headers.first(name)?.nonsensitive_bytes()?).ok()
}
