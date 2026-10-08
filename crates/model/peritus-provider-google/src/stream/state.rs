//! Dialect dispatch, exact event deduplication, and normalized envelope ownership.

use std::collections::{BTreeMap, VecDeque};

use peritus_model_protocol::{
    CacheObservation, EventEnvelope, EventId, ItemId, ModelEvent, ProtocolLimits, ProviderName,
    StreamFragment, ToolCallId, WireDialect,
};
use peritus_provider_core::{
    HttpHeaders, ProviderCoreError, SseFrame, SseItem, healing::JsonCompletionCursor,
};
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

struct DeferredEvent {
    event: ModelEvent,
    digest: Sha256Digest,
    provider_event_id: Option<EventId>,
}

enum DeferredEmission {
    Event(DeferredEvent),
    Completion {
        cursor: JsonCompletionCursor,
        digest: Sha256Digest,
        provider_event_id: Option<EventId>,
    },
}

impl DeferredEmission {
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Event(DeferredEvent {
                event: ModelEvent::ResponseCompleted
                    | ModelEvent::ResponseFailed(_)
                    | ModelEvent::ResponseCancelled,
                ..
            })
        )
    }
}

pub(super) struct NormalizeState {
    pub(super) provider: ProviderName,
    dialect: DialectState,
    limits: ProtocolLimits,
    sequence: u64,
    pending: VecDeque<EventEnvelope>,
    deferred: VecDeque<DeferredEmission>,
    deferred_events: usize,
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
            deferred: VecDeque::new(),
            deferred_events: 0,
            staged_terminal: None,
            seen: BTreeMap::new(),
            active_event_id: None,
            last_cache: None,
            metadata: metadata_events(headers, limits)?,
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

    pub(super) fn is_terminal(&self) -> bool {
        self.staged_terminal.is_some() || self.deferred.iter().any(DeferredEmission::is_terminal)
    }

    pub(super) const fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    pub(super) fn clear_deferred(&mut self) {
        self.deferred.clear();
        self.deferred_events = 0;
    }

    pub(super) fn cancel_unpublished(&mut self) -> Result<bool, ProviderCoreError> {
        let replaceable = self.staged_terminal.as_ref().is_none_or(|terminal| {
            matches!(terminal.event(), ModelEvent::ResponseCompleted)
        });
        if !replaceable {
            return Ok(false);
        }
        self.clear_deferred();
        self.push_synthetic(ModelEvent::ResponseCancelled)?;
        Ok(true)
    }

    pub(super) const fn has_observed_semantics(&self) -> bool {
        self.observed_semantics
    }

    pub(super) const fn limits(&self) -> ProtocolLimits {
        self.limits
    }

    pub(super) fn emit_structured_progress(
        &mut self,
        item_id: &ItemId,
        revision: &mut u64,
        bytes: &[u8],
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if bytes.is_empty() {
            return self.emit_empty_progress(digest, event_id);
        }
        let maximum = self.limits.max_event_bytes();
        if maximum == 0 {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google structured progress fragment bound was zero",
            ));
        }
        for chunk in bytes.chunks(maximum) {
            *revision = revision.checked_add(1).ok_or_else(|| {
                ProviderCoreError::limit_exceeded(
                    "google_stream",
                    "Google structured progress revision overflowed",
                )
            })?;
            let fragment = StreamFragment::new(chunk.to_vec(), self.limits)
                .map_err(|_| invalid("Google structured progress fragment exceeds its bound"))?;
            self.emit(
                ModelEvent::StructuredOutputProgress {
                    item_id: item_id.clone(),
                    revision: *revision,
                    fragment,
                },
                digest,
                event_id,
            )?;
        }
        Ok(())
    }

    pub(super) fn emit_tool_progress(
        &mut self,
        call_id: &ToolCallId,
        revision: &mut u64,
        bytes: &[u8],
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        if bytes.is_empty() {
            return self.emit_empty_progress(digest, event_id);
        }
        let maximum = self.limits.max_event_bytes();
        if maximum == 0 {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google tool progress fragment bound was zero",
            ));
        }
        for chunk in bytes.chunks(maximum) {
            *revision = revision.checked_add(1).ok_or_else(|| {
                ProviderCoreError::limit_exceeded(
                    "google_stream",
                    "Google tool progress revision overflowed",
                )
            })?;
            let fragment = StreamFragment::new(chunk.to_vec(), self.limits)
                .map_err(|_| invalid("Google tool progress fragment exceeds its bound"))?;
            self.emit(
                ModelEvent::ToolArgumentProgress {
                    call_id: call_id.clone(),
                    revision: *revision,
                    fragment,
                },
                digest,
                event_id,
            )?;
        }
        Ok(())
    }

    fn emit_empty_progress(
        &mut self,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        self.observed_semantics = true;
        self.emit(ModelEvent::Heartbeat, digest, event_id)
    }

    pub(super) fn emit_empty_text_observation(
        &mut self,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        self.emit_empty_progress(digest, event_id)
    }

    pub(super) fn defer_completion(
        &mut self,
        cursor: JsonCompletionCursor,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let remaining = cursor.remaining_events();
        self.admit_deferred(remaining)?;
        let provider_event_id = event_id.and_then(|_| self.active_event_id.take());
        self.observed_semantics = true;
        self.deferred.push_back(DeferredEmission::Completion {
            cursor,
            digest,
            provider_event_id,
        });
        self.deferred_events = self
            .deferred_events
            .checked_add(remaining)
            .ok_or_else(|| {
                ProviderCoreError::limit_exceeded(
                    "google_stream",
                    "Google deferred completion event count overflowed",
                )
            })?;
        Ok(())
    }

    pub(super) fn resume_deferred(&mut self) -> Result<bool, ProviderCoreError> {
        loop {
            let Some(emission) = self.deferred.pop_front() else { return Ok(false) };
            match emission {
                DeferredEmission::Event(event) => {
                    self.deferred_events = self
                        .deferred_events
                        .checked_sub(1)
                        .ok_or_else(|| invalid("Google deferred event count underflowed"))?;
                    self.emit_now(event.event, event.digest, event.provider_event_id)?;
                    return Ok(true);
                }
                DeferredEmission::Completion {
                    mut cursor,
                    digest,
                    mut provider_event_id,
                } => {
                    let Some(event) = cursor.next_event()? else { continue };
                    self.deferred_events = self
                        .deferred_events
                        .checked_sub(1)
                        .ok_or_else(|| invalid("Google deferred event count underflowed"))?;
                    if cursor.remaining_events() > 0 {
                        self.deferred.push_front(DeferredEmission::Completion {
                            cursor,
                            digest,
                            provider_event_id: None,
                        });
                    }
                    self.emit_now(event, digest, provider_event_id.take())?;
                    return Ok(true);
                }
            }
        }
    }

    pub(super) fn emit_replay(
        &mut self,
        item_id: &ItemId,
        bytes: &[u8],
        digest: Sha256Digest,
        event_id: Option<&str>,
        trailing_events: usize,
    ) -> Result<(), ProviderCoreError> {
        let fragment_bytes = self.admit_replay(bytes, trailing_events)?;
        for chunk in bytes.chunks(fragment_bytes) {
            let fragment = StreamFragment::new(chunk.to_vec(), self.limits)
                .map_err(|_| invalid("Google reasoning replay fragment exceeds its bound"))?;
            self.emit(
                ModelEvent::ReasoningReplayDelta { item_id: item_id.clone(), fragment },
                digest,
                event_id,
            )?;
        }
        Ok(())
    }

    pub(super) fn emit_deferred_replay(
        &mut self,
        item_id: &ItemId,
        bytes: &[u8],
        digest: Sha256Digest,
        event_id: Option<&str>,
        trailing_events: usize,
    ) -> Result<(), ProviderCoreError> {
        let fragment_bytes = self.admit_replay(bytes, trailing_events)?;
        let mut provider_event_id = event_id
            .map(|value| EventId::new(value.to_owned()))
            .transpose()
            .map_err(|_| invalid("Google deferred replay event identity is invalid"))?;
        for chunk in bytes.chunks(fragment_bytes) {
            let fragment = StreamFragment::new(chunk.to_vec(), self.limits)
                .map_err(|_| invalid("Google reasoning replay fragment exceeds its bound"))?;
            self.emit_with_event_id(
                ModelEvent::ReasoningReplayDelta { item_id: item_id.clone(), fragment },
                digest,
                provider_event_id.take(),
            )?;
        }
        Ok(())
    }

    pub(super) fn emit_deferred(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let provider_event_id = event_id
            .map(|value| EventId::new(value.to_owned()))
            .transpose()
            .map_err(|_| invalid("Google deferred event identity is invalid"))?;
        self.emit_with_event_id(event, digest, provider_event_id)
    }

    fn admit_replay(
        &self,
        bytes: &[u8],
        trailing_events: usize,
    ) -> Result<usize, ProviderCoreError> {
        let maximum = self.limits.max_extension_bytes().min(self.limits.max_output_bytes());
        if bytes.is_empty() || bytes.len() > maximum {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google reasoning replay exceeds its aggregate bound",
            ));
        }
        let fragment_bytes = self.limits.max_event_bytes();
        let fragment_adjustment = fragment_bytes.checked_sub(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google reasoning replay fragment bound was zero",
            )
        })?;
        let fragments = bytes
            .len()
            .checked_add(fragment_adjustment)
            .map(|length| length / fragment_bytes)
            .ok_or_else(|| {
                ProviderCoreError::limit_exceeded(
                    "google_stream",
                    "Google reasoning replay fragment count overflowed",
                )
            })?;
        let future_events = fragments.checked_add(trailing_events).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google reasoning replay event count overflowed",
            )
        })?;
        let emitted = usize::try_from(self.sequence).unwrap_or(usize::MAX);
        if future_events > self.limits.max_events().saturating_sub(emitted) {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google reasoning replay exceeds the selected event bound",
            ));
        }
        Ok(fragment_bytes)
    }

    fn admit_deferred(&self, additional: usize) -> Result<(), ProviderCoreError> {
        let emitted = usize::try_from(self.sequence).unwrap_or(usize::MAX);
        let projected = emitted
            .checked_add(self.deferred_events)
            .and_then(|count| count.checked_add(additional))
            .ok_or_else(|| {
                ProviderCoreError::limit_exceeded(
                    "google_stream",
                    "Google deferred event count overflowed",
                )
            })?;
        if projected > self.limits.max_events() {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google deferred output exceeds the selected event bound",
            ));
        }
        Ok(())
    }

    fn queue_event(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        provider_event_id: Option<EventId>,
    ) -> Result<(), ProviderCoreError> {
        self.admit_deferred(1)?;
        self.deferred.push_back(DeferredEmission::Event(DeferredEvent {
            event,
            digest,
            provider_event_id,
        }));
        self.deferred_events = self.deferred_events.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "Google deferred event count overflowed",
            )
        })?;
        Ok(())
    }

    pub(super) fn emit(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        event_id: Option<&str>,
    ) -> Result<(), ProviderCoreError> {
        let provider_event_id = event_id.and_then(|_| self.active_event_id.take());
        self.emit_with_event_id(event, digest, provider_event_id)
    }

    fn emit_with_event_id(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        provider_event_id: Option<EventId>,
    ) -> Result<(), ProviderCoreError> {
        if let ModelEvent::Cache(observation) = &event {
            if self.last_cache.as_ref() == Some(observation) {
                return Ok(());
            }
            self.last_cache = Some(observation.clone());
        }
        if self.staged_terminal.is_some()
            && !matches!(
                event,
                ModelEvent::ResponseCompleted
                    | ModelEvent::ResponseFailed(_)
                    | ModelEvent::ResponseCancelled
            )
        {
            return Err(invalid("Google event followed a terminal event"));
        }
        self.observed_semantics |= !matches!(event, ModelEvent::Heartbeat);
        if !self.deferred.is_empty() {
            return self.queue_event(event, digest, provider_event_id);
        }
        self.emit_now(event, digest, provider_event_id)
    }

    fn emit_now(
        &mut self,
        event: ModelEvent,
        digest: Sha256Digest,
        provider_event_id: Option<EventId>,
    ) -> Result<(), ProviderCoreError> {
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
        let sequence = self.sequence.checked_add(1).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "google_stream",
                "normalized event sequence overflowed",
            )
        })?;
        if usize::try_from(sequence).unwrap_or(usize::MAX) > self.limits.max_events() {
            return Err(ProviderCoreError::limit_exceeded(
                "google_stream",
                "normalized events exceed the selected event bound",
            ));
        }
        self.sequence = sequence;
        Ok(sequence)
    }

    fn emit_synthetic(&mut self, event: ModelEvent) -> Result<(), ProviderCoreError> {
        self.emit(event, peritus_types::Sha256Digest::new([0; 32]), None)
    }
}
