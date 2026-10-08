//! Anthropic SSE event dispatch, deduplication, and normalized envelope ownership.

use std::collections::{BTreeMap, VecDeque};

use peritus_model_protocol::{
    CanonicalJson, EventEnvelope, EventId, ExtensionName, ItemId, JsonBounds, ModelEvent,
    ProtocolLimits, ProviderExtension, ProviderName, ResponseId, StreamFragment,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, SseFrame, SseItem};
use serde_json::Value;

use super::value::{ReplayBytes, ReplayKind, invalid, limit, metadata_events, owned_string};

struct ExactReplayIndex {
    entries: BTreeMap<String, peritus_types::Sha256Digest>,
    identity_bytes: usize,
    max_entries: usize,
    max_identity_bytes: usize,
}

impl ExactReplayIndex {
    const fn new(max_entries: usize, max_identity_bytes: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            identity_bytes: 0,
            max_entries,
            max_identity_bytes,
        }
    }

    fn observe(
        &mut self,
        id: &str,
        digest: peritus_types::Sha256Digest,
    ) -> Result<bool, ProviderCoreError> {
        if let Some(previous) = self.entries.get(id) {
            if *previous == digest {
                return Ok(true);
            }
            return Err(invalid(
                "Anthropic SSE event identity was reused with different bytes",
            ));
        }
        if self.entries.len() >= self.max_entries {
            return Err(limit("Anthropic replay index exceeded the selected event bound"));
        }
        let next_identity_bytes = self
            .identity_bytes
            .checked_add(id.len())
            .ok_or_else(|| limit("Anthropic replay identity byte count overflowed"))?;
        if next_identity_bytes > self.max_identity_bytes {
            return Err(limit(
                "Anthropic replay identities exceeded the selected response-body bound",
            ));
        }
        let identity = owned_string(id, "Anthropic replay identity capacity is unavailable")?;
        self.entries.insert(identity, digest);
        self.identity_bytes = next_identity_bytes;
        Ok(false)
    }
}

struct DeferredReplay {
    item_id: ItemId,
    bytes: ReplayBytes,
    digest: peritus_types::Sha256Digest,
    event_id: Option<String>,
}

pub(super) enum Phase {
    AwaitingStart,
    Content,
    MessageDelta,
    Stopped,
}

pub(super) enum ActiveBlock {
    Text {
        item_id: peritus_model_protocol::ItemId,
    },
    Tool {
        item_id: peritus_model_protocol::ItemId,
        call_id: peritus_model_protocol::ToolCallId,
        arguments: peritus_provider_core::healing::ToolArgumentBuffer,
    },
    Thinking {
        item_id: peritus_model_protocol::ItemId,
        signature: bool,
    },
    Redacted {
        item_id: peritus_model_protocol::ItemId,
    },
}

pub(super) struct UsageState {
    pub(super) input: Option<u64>,
    pub(super) cache_read: Option<u64>,
    pub(super) cache_creation: Option<u64>,
    pub(super) output: Option<u64>,
}

pub(super) struct NormalizeState {
    pub(super) provider: ProviderName,
    pub(super) phase: Phase,
    pub(super) response_id: Option<ResponseId>,
    pub(super) blocks: BTreeMap<u32, ActiveBlock>,
    pub(super) next_block: u32,
    pub(super) usage: UsageState,
    limits: ProtocolLimits,
    sequence: u64,
    pending: VecDeque<EventEnvelope>,
    replay_index: ExactReplayIndex,
    deferred_replay: Option<DeferredReplay>,
    metadata: Vec<ModelEvent>,
    terminal: bool,
    observed_semantics: bool,
}

impl NormalizeState {
    #[cfg(test)]
    pub(super) fn new(
        provider: ProviderName,
        headers: &HttpHeaders,
    ) -> Result<Self, ProviderCoreError> {
        Self::with_limits(
            provider,
            headers,
            ProtocolLimits::PRODUCTION,
            peritus_provider_core::HttpLimits::PRODUCTION.max_response_body_bytes(),
        )
    }

    pub(super) fn with_limits(
        provider: ProviderName,
        headers: &HttpHeaders,
        limits: ProtocolLimits,
        replay_identity_bytes: usize,
    ) -> Result<Self, ProviderCoreError> {
        Ok(Self {
            provider,
            phase: Phase::AwaitingStart,
            response_id: None,
            blocks: BTreeMap::new(),
            next_block: 0,
            usage: UsageState { input: None, cache_read: None, cache_creation: None, output: None },
            limits,
            sequence: 0,
            pending: VecDeque::new(),
            replay_index: ExactReplayIndex::new(limits.max_events(), replay_identity_bytes),
            deferred_replay: None,
            metadata: metadata_events(headers, limits)?,
            terminal: false,
            observed_semantics: false,
        })
    }

    pub(super) fn process(&mut self, item: SseItem) -> Result<(), ProviderCoreError> {
        match item {
            SseItem::Comment(_) => Ok(()),
            SseItem::Done => {
                Err(invalid("Anthropic Messages emitted an unsupported DONE sentinel"))
            }
            SseItem::Event(frame) => self.process_frame(&frame),
        }
    }

    pub(super) fn push_synthetic(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.push(event, peritus_types::Sha256Digest::new([0; 32]), None)
    }

    pub(super) fn take_pending(&mut self) -> VecDeque<EventEnvelope> {
        core::mem::take(&mut self.pending)
    }

    pub(super) const fn is_terminal(&self) -> bool {
        self.terminal
    }

    pub(super) const fn has_observed_semantics(&self) -> bool {
        self.observed_semantics
    }

    pub(super) const fn limits(&self) -> ProtocolLimits {
        self.limits
    }

    pub(super) const fn has_deferred_replay(&self) -> bool {
        self.deferred_replay.is_some()
    }

    pub(super) fn defer_replay(
        &mut self,
        item_id: ItemId,
        kind: ReplayKind,
        value: &str,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if self.deferred_replay.is_some() {
            return Err(invalid("Anthropic reasoning replay overlapped another replay"));
        }
        let maximum = self.limits.max_extension_bytes().min(self.limits.max_output_bytes());
        let bytes = ReplayBytes::new(kind, value, maximum)?;
        let fragments = bytes.fragment_count(self.limits.max_event_bytes())?;
        let emitted = usize::try_from(self.sequence).unwrap_or(usize::MAX);
        if fragments > self.limits.max_events().saturating_sub(emitted) {
            return Err(limit("Anthropic reasoning replay exceeded the selected event bound"));
        }
        let event_id = event_id
            .map(|value| {
                owned_string(value, "Anthropic deferred event identity capacity is unavailable")
            })
            .transpose()?;
        self.deferred_replay = Some(DeferredReplay { item_id, bytes, digest, event_id });
        Ok(())
    }

    pub(super) fn emit_deferred_replay(&mut self) -> Result<bool, ProviderCoreError> {
        let Some(mut replay) = self.deferred_replay.take() else { return Ok(false) };
        let Some(bytes) = replay.bytes.next_chunk(self.limits.max_event_bytes())? else {
            return Ok(false);
        };
        let fragment = StreamFragment::new(bytes, self.limits)
            .map_err(|_| invalid("Anthropic reasoning replay fragment is invalid"))?;
        let event_id = replay.event_id.take();
        self.push(
            ModelEvent::ReasoningReplayDelta {
                item_id: replay.item_id.clone(),
                fragment,
            },
            replay.digest,
            event_id.as_deref(),
        )?;
        self.deferred_replay = Some(replay);
        Ok(true)
    }

    pub(super) fn clear_deferred_replay(&mut self) {
        self.deferred_replay = None;
    }

    pub(super) fn emit(
        &mut self,
        event: ModelEvent,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        self.push(event, digest, event_id)
    }

    pub(super) fn drain_metadata(
        &mut self,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        for event in core::mem::take(&mut self.metadata) {
            self.push(event, digest, event_id)?;
        }
        Ok(())
    }

    fn process_frame(&mut self, frame: &SseFrame) -> Result<(), ProviderCoreError> {
        if self.terminal {
            return Err(invalid("Anthropic event followed a terminal event"));
        }
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        if let Some(id) = frame.id()
            && self.replay_index.observe(id, digest)?
        {
                return Ok(());
        }
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| invalid("Anthropic SSE data is not valid JSON"))?;
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Anthropic SSE event type is missing"))?;
        if frame.event().is_some_and(|event| event != kind) {
            return Err(invalid("Anthropic SSE event name and payload type disagree"));
        }
        match kind {
            "message_start" => super::message::start(self, &value, digest, frame.id()),
            "content_block_start" => super::content::start(self, &value, digest, frame.id()),
            "content_block_delta" => super::content::delta(self, &value, digest, frame.id()),
            "content_block_stop" => super::content::stop(self, &value, digest, frame.id()),
            "message_delta" => super::message::delta(self, &value, digest, frame.id()),
            "message_stop" => super::message::stop(self, &value, digest, frame.id()),
            "ping" => self.push(ModelEvent::Heartbeat, digest, frame.id()),
            "error" => super::message::error(self, &value, digest, frame.id()),
            unknown if correctness_critical(unknown) => {
                Err(invalid("Anthropic emitted an unknown correctness-critical event"))
            }
            unknown => self.ancillary(unknown, frame.data(), digest, frame.id()),
        }
    }

    fn ancillary(
        &mut self,
        kind: &str,
        data: &str,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if kind.is_empty()
            || kind.len() > 64
            || !kind.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid("Anthropic ancillary event name is unsafe"));
        }
        let name = ExtensionName::new(format!("anthropic.{kind}"))
            .map_err(|_| invalid("Anthropic ancillary event name is invalid"))?;
        let value = CanonicalJson::parse(data, JsonBounds::value(self.limits))
            .map_err(|_| invalid("Anthropic ancillary event exceeds JSON bounds"))?;
        self.push(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)), digest, event_id)
    }

    fn push(
        &mut self,
        event: ModelEvent,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "anthropic_stream",
                "normalized event sequence overflowed",
            )
        })?;
        if usize::try_from(sequence).map_or(true, |count| count > self.limits.max_events()) {
            return Err(limit("Anthropic normalized events exceeded the selected event bound"));
        }
        let provider_event_id = event_id
            .map(|id| {
                owned_string(id, "Anthropic event identity capacity is unavailable")
                    .and_then(|value| {
                        EventId::new(value)
                            .map_err(|_| invalid("Anthropic SSE event ID is invalid"))
                    })
            })
            .transpose()
            ?;
        let observed_semantics = !matches!(event, ModelEvent::Heartbeat);
        let terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        self.pending
            .try_reserve(1)
            .map_err(|_| limit("Anthropic normalized event capacity is unavailable"))?;
        let envelope = EventEnvelope::new(sequence, None, provider_event_id, digest, event)
            .map_err(|_| invalid("normalized Anthropic event envelope is invalid"))?;
        self.sequence = sequence;
        self.observed_semantics |= observed_semantics;
        self.terminal = terminal;
        self.pending.push_back(envelope);
        Ok(())
    }
}

fn correctness_critical(kind: &str) -> bool {
    kind.starts_with("message_") || kind.starts_with("content_block_")
}
