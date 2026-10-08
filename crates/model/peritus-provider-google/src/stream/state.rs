//! Dialect dispatch, exact event deduplication, and normalized envelope ownership.

use std::collections::{BTreeMap, VecDeque};

use peritus_model_protocol::{
    CacheObservation, EventEnvelope, EventId, ModelEvent, ProtocolLimits, ProviderName,
    WireDialect,
};
use peritus_provider_core::{HttpHeaders, ProviderCoreError, SseFrame, SseItem};
use peritus_types::Sha256Digest;
use serde_json::Value;

use super::generate::GenerateState;
use super::interactions::InteractionState;
use super::value::{invalid, metadata_events};

enum DialectState {
    Interactions(InteractionState),
    Generate(GenerateState),
    Vacant,
}

pub(super) struct NormalizeState {
    pub(super) provider: ProviderName,
    dialect: DialectState,
    limits: ProtocolLimits,
    sequence: u64,
    pending: VecDeque<EventEnvelope>,
    staged_terminal: Option<EventEnvelope>,
    seen: BTreeMap<EventId, Sha256Digest>,
    active_event_id: Option<EventId>,
    last_cache: Option<CacheObservation>,
    metadata: Vec<ModelEvent>,
    observed_semantics: bool,
}

impl NormalizeState {
    pub(super) fn new(
        provider: ProviderName,
        dialect: WireDialect,
        structured: bool,
        tool_controls: crate::request::ToolControls,
        headers: &HttpHeaders,
        limits: ProtocolLimits,
    ) -> Result<Self, ProviderCoreError> {
        let dialect = match dialect {
            WireDialect::GeminiInteractionsV1 => {
                DialectState::Interactions(InteractionState::new(structured, tool_controls))
            }
            WireDialect::GeminiGenerateContentV1 => {
                DialectState::Generate(GenerateState::new(structured, tool_controls))
            }
            _ => return Err(invalid("Google stream selected a non-Google dialect")),
        };
        Ok(Self {
            provider,
            dialect,
            limits,
            sequence: 0,
            pending: VecDeque::new(),
            staged_terminal: None,
            seen: BTreeMap::new(),
            active_event_id: None,
            last_cache: None,
            metadata: metadata_events(headers)?,
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

    pub(super) fn take_staged_terminal(&mut self) -> Option<EventEnvelope> {
        self.staged_terminal.take()
    }

    pub(super) const fn has_staged_terminal(&self) -> bool {
        self.staged_terminal.is_some()
    }

    pub(super) fn cancel_unpublished(&mut self) -> Result<bool, ProviderCoreError> {
        let replaceable = self.staged_terminal.as_ref().is_none_or(|terminal| {
            matches!(terminal.event(), ModelEvent::ResponseCompleted)
        });
        if !replaceable {
            return Ok(false);
        }
        self.push_synthetic(ModelEvent::ResponseCancelled)?;
        Ok(true)
    }

    pub(super) const fn has_observed_semantics(&self) -> bool {
        self.observed_semantics
    }

    pub(super) fn emit(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if let ModelEvent::Cache(observation) = &event {
            if self.last_cache.as_ref() == Some(observation) {
                return Ok(());
            }
            self.last_cache = Some(observation.clone());
        }
        let terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        if self.staged_terminal.is_some() && !terminal {
            return Err(invalid("Google event followed a terminal event"));
        }
        let sequence = if terminal {
            if let Some(staged) = &self.staged_terminal {
                staged.sequence()
            } else {
                self.next_sequence()?
            }
        } else {
            self.next_sequence()?
        };
        let provider_event_id = event_id.and_then(|_| self.active_event_id.take());
        self.observed_semantics |= !matches!(event, ModelEvent::Heartbeat);
        let envelope = EventEnvelope::new(sequence, None, provider_event_id, digest, event)
            .map_err(|_| invalid("normalized Google event envelope is invalid"))?;
        if terminal {
            let replace_completion = self.staged_terminal.as_ref().is_some_and(|staged| {
                matches!(staged.event(), ModelEvent::ResponseCompleted)
                    && !matches!(envelope.event(), ModelEvent::ResponseCompleted)
            });
            if self.staged_terminal.is_none() || replace_completion {
                self.staged_terminal = Some(envelope);
            }
        } else {
            self.pending.push_back(envelope);
        }
        Ok(())
    }

    pub(super) fn drain_metadata(
        &mut self,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        for event in core::mem::take(&mut self.metadata) {
            self.emit(event, digest, event_id)?;
        }
        Ok(())
    }

    fn process_frame(&mut self, frame: &SseFrame) -> Result<(), ProviderCoreError> {
        if self.staged_terminal.is_some() {
            return Err(invalid("Google frame followed a terminal event"));
        }
        let digest = peritus_codec::sha256(frame.data().as_bytes());
        let event_id = frame
            .id()
            .map(|id| EventId::new(id.to_owned()))
            .transpose()
            .map_err(|_| invalid("Google SSE event ID is invalid"))?;
        if let Some(id) = &event_id {
            match self.seen.get(id) {
                Some(previous) if *previous == digest => return Ok(()),
                Some(_) => {
                    return Err(invalid("Google reused an SSE event ID with different data"));
                }
                None if self.seen.len() >= self.limits.max_events() => {
                    return Err(ProviderCoreError::limit_exceeded(
                        "google_stream",
                        "Google event deduplication index exceeded the selected event bound",
                    ));
                }
                None => {
                    self.seen.insert(id.clone(), digest);
                }
            }
        }
        let value: Value = serde_json::from_str(frame.data())
            .map_err(|_| invalid("Google SSE data is not valid JSON"))?;
        self.active_event_id = event_id;
        let mut dialect = core::mem::replace(&mut self.dialect, DialectState::Vacant);
        let result = match &mut dialect {
            DialectState::Interactions(state) => state.process(self, frame, &value, digest),
            DialectState::Generate(state) => state.process(self, frame, &value, digest),
            DialectState::Vacant => Err(invalid("Google stream decoder state is unavailable")),
        };
        self.dialect = dialect;
        self.active_event_id = None;
        result
    }

    fn next_sequence(&mut self) -> Result<u64, ProviderCoreError> {
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "normalized event sequence overflowed",
            )
        })?;
        Ok(self.sequence)
    }

    fn emit_synthetic(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.emit(event, peritus_types::Sha256Digest::new([0; 32]), None)
    }
}
