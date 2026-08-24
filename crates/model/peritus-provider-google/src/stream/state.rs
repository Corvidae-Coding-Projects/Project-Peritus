//! Dialect dispatch, exact event deduplication, and normalized envelope ownership.

use std::collections::{BTreeMap, VecDeque};

use peritus_model_protocol::{EventEnvelope, EventId, ModelEvent, ProviderName, WireDialect};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, SseFrame, SseItem};
use serde_json::Value;

use super::generate::GenerateState;
use super::interactions::InteractionState;
use super::value::{invalid, metadata_events};

enum DialectState {
    Interactions(InteractionState),
    Generate(GenerateState),
}

pub(super) struct NormalizeState {
    pub(super) provider: ProviderName,
    dialect: DialectState,
    sequence: u64,
    pending: VecDeque<EventEnvelope>,
    seen: BTreeMap<String, [u8; 32]>,
    metadata: Vec<ModelEvent>,
    terminal: bool,
    observed_semantics: bool,
}

impl NormalizeState {
    pub(super) fn new(
        provider: ProviderName,
        dialect: WireDialect,
        structured: bool,
        headers: &HttpHeaders,
    ) -> Result<Self, ProviderCoreError> {
        let dialect = match dialect {
            WireDialect::GeminiInteractionsV1 => {
                DialectState::Interactions(InteractionState::new(structured))
            }
            WireDialect::GeminiGenerateContentV1 => {
                DialectState::Generate(GenerateState::new(structured))
            }
            _ => return Err(invalid("Google stream selected a non-Google dialect")),
        };
        Ok(Self {
            provider,
            dialect,
            sequence: 0,
            pending: VecDeque::new(),
            seen: BTreeMap::new(),
            metadata: metadata_events(headers)?,
            terminal: false,
            observed_semantics: false,
        })
    }

    pub(super) fn process(&mut self, item: SseItem) -> Result<(), ProviderCoreError> {
        match item {
            SseItem::Comment(_) => self.emit_synthetic(ModelEvent::Heartbeat),
            SseItem::Done => Err(invalid("Google stable-v1 emitted an unsupported DONE sentinel")),
            SseItem::Event(frame) => self.process_frame(&frame),
        }
    }

    pub(super) fn push_synthetic(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.emit(event, peritus_types::Sha256Digest::new([0; 32]), None)
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
        if self.terminal {
            return Err(invalid("Google event followed a terminal event"));
        }
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "normalized event sequence overflowed",
            )
        })?;
        let provider_event_id = event_id
            .map(|id| EventId::new(id.to_owned()))
            .transpose()
            .map_err(|_| invalid("Google SSE event ID is invalid"))?;
        self.observed_semantics |= !matches!(event, ModelEvent::Heartbeat);
        self.terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        let envelope = EventEnvelope::new(self.sequence, None, provider_event_id, digest, event)
            .map_err(|_| invalid("normalized Google event envelope is invalid"))?;
        self.pending.push_back(envelope);
        Ok(())
    }

    pub(super) fn drain_metadata(
        &mut self,
        digest: peritus_types::Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        for event in core::mem::take(&mut self.metadata) {
            self.emit(event, digest, event_id)?;
        }
        Ok(())
    }

    fn process_frame(&mut self, frame: &SseFrame) -> Result<(), ProviderCoreError> {
        if self.terminal {
            return Err(invalid("Google frame followed a terminal event"));
        }
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        if let Some(id) = frame.id() {
            match self.seen.get(id) {
                Some(previous) if *previous == digest.into_bytes() => return Ok(()),
                Some(_) => {
                    return Err(invalid("Google reused an SSE event ID with different data"));
                }
                None if self.seen.len() >= 4_096 => {
                    return Err(ProviderCoreError::limit_exceeded(
                        "google_stream",
                        "Google event deduplication set exceeded its bound",
                    ));
                }
                None => {
                    self.seen.insert(id.to_owned(), digest.into_bytes());
                }
            }
        }
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| invalid("Google SSE data is not valid JSON"))?;
        let mut dialect = core::mem::replace(
            &mut self.dialect,
            DialectState::Generate(GenerateState::new(false)),
        );
        let result = match &mut dialect {
            DialectState::Interactions(state) => state.process(self, frame, &value, digest),
            DialectState::Generate(state) => state.process(self, frame, &value, digest),
        };
        self.dialect = dialect;
        result
    }

    fn emit_synthetic(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.emit(event, peritus_types::Sha256Digest::new([0; 32]), None)
    }
}
