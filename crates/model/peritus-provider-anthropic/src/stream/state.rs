//! Anthropic SSE event dispatch, deduplication, and normalized envelope ownership.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use peritus_model_protocol::{
    CanonicalJson, EventEnvelope, EventId, ExtensionName, JsonBounds, ModelEvent, ProtocolLimits,
    ProviderExtension, ProviderName, ResponseId,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, SseFrame, SseItem};
use serde_json::Value;

use super::value::{invalid, metadata_events};

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
        arguments: Vec<u8>,
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
    sequence: u64,
    pending: VecDeque<EventEnvelope>,
    seen: BTreeSet<(String, [u8; 32])>,
    metadata: Vec<ModelEvent>,
    terminal: bool,
    observed_semantics: bool,
}

impl NormalizeState {
    pub(super) fn new(
        provider: ProviderName,
        headers: &HttpHeaders,
    ) -> Result<Self, ProviderCoreError> {
        Ok(Self {
            provider,
            phase: Phase::AwaitingStart,
            response_id: None,
            blocks: BTreeMap::new(),
            next_block: 0,
            usage: UsageState { input: None, cache_read: None, cache_creation: None, output: None },
            sequence: 0,
            pending: VecDeque::new(),
            seen: BTreeSet::new(),
            metadata: metadata_events(headers)?,
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
        if let Some(id) = frame.id() {
            let key = (id.to_owned(), digest.into_bytes());
            if self.seen.contains(&key) {
                return Ok(());
            }
            if self.seen.len() >= 4_096 {
                return Err(ProviderCoreError::limit_exceeded(
                    "anthropic_stream",
                    "Anthropic event deduplication set exceeded its bound",
                ));
            }
            self.seen.insert(key);
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
        let value = CanonicalJson::parse(data, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|_| invalid("Anthropic ancillary event exceeds JSON bounds"))?;
        self.push(ModelEvent::ProviderEvent(ProviderExtension::new(name, value)), digest, event_id)
    }

    fn push(
        &mut self,
        event: ModelEvent,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "anthropic_stream",
                "normalized event sequence overflowed",
            )
        })?;
        let provider_event_id = event_id
            .map(|id| EventId::new(id.to_owned()))
            .transpose()
            .map_err(|_| invalid("Anthropic SSE event ID is invalid"))?;
        self.observed_semantics |= !matches!(event, ModelEvent::Heartbeat);
        self.terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        let envelope = EventEnvelope::new(self.sequence, None, provider_event_id, digest, event)
            .map_err(|_| invalid("normalized Anthropic event envelope is invalid"))?;
        self.pending.push_back(envelope);
        Ok(())
    }
}

fn correctness_critical(kind: &str) -> bool {
    kind.starts_with("message_") || kind.starts_with("content_block_")
}
