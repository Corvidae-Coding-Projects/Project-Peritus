//! Bounded `OpenAI` Responses SSE ownership and normalized event emission.

mod decode;
pub mod metadata;
mod output;
mod state;
mod terminal;

use core::fmt;
use std::collections::{BTreeMap, VecDeque};
#[cfg(test)]
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ModelName, OutcomeCertainty, ProtocolLimits,
    ProviderName, ResponseId, Retryability, TransportPhase,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, FramingLimits, ModelStream, ProviderCoreError,
    ProviderCoreErrorKind, SseItem, SseParser,
};
use peritus_types::ProviderProfileId;

use crate::error;

#[allow(
    clippy::struct_excessive_bools,
    reason = "decoder lifecycle and request properties are independent audited state"
)]
pub struct OpenAiStream {
    body: Box<dyn ByteStream>,
    parser: SseParser,
    pending: VecDeque<EventEnvelope>,
    staged_terminal: Option<EventEnvelope>,
    local_sequence: u64,
    terminal: bool,
    body_finished: bool,
    provider: ProviderName,
    expected_model: ModelName,
    structured_output: bool,
    limits: ProtocolLimits,
    state: state::ResponseState,
    metadata: metadata::ResponseMetadata,
    track_background: bool,
    background_responses: BackgroundResponseRegistry,
    restored_partial: Vec<EventEnvelope>,
}

/// Decoder checkpoint rebuilt from the exact durable normalized prefix.
pub(crate) struct OpenAiResumeState {
    local_sequence: u64,
    provider_sequence: u64,
    profile_id: ProviderProfileId,
    profile_revision: u64,
    expected_model: ModelName,
    state: state::ResponseState,
    partial: Vec<EventEnvelope>,
}

impl OpenAiResumeState {
    pub(crate) fn restore(
        prefix: &[EventEnvelope],
        response_id: &ResponseId,
        provider_sequence: u64,
        profile_id: ProviderProfileId,
        profile_revision: u64,
        expected_model: &ModelName,
        limits: ProtocolLimits,
    ) -> Result<Option<Self>, ProviderCoreError> {
        let cursor_index = prefix
            .iter()
            .rposition(|envelope| envelope.provider_sequence().is_some())
            .ok_or_else(|| error::malformed("persisted OpenAI prefix has no provider cursor"))?;
        let cursor = &prefix[cursor_index];
        if cursor.provider_sequence() != Some(provider_sequence) {
            return Err(error::malformed(
                "persisted OpenAI prefix does not end at its continuation cursor",
            ));
        }
        let partial_start = prefix[cursor_index + 1..]
            .iter()
            .position(|envelope| !matches!(envelope.event(), ModelEvent::Heartbeat))
            .map_or(prefix.len(), |offset| cursor_index + 1 + offset);
        let partial = &prefix[partial_start..];
        if partial.iter().any(|envelope| {
            envelope.provider_sequence().is_some() || envelope.provider_event_id().is_some()
        }) {
            return Err(error::malformed(
                "persisted OpenAI partial frame contains a second provider cursor",
            ));
        }
        if let Some(first) = partial.first() {
            if first.provider_digest() == cursor.provider_digest()
                || partial
                    .iter()
                    .any(|envelope| envelope.provider_digest() != first.provider_digest())
            {
                return Ok(None);
            }
        }
        let Some(state) = state::ResponseState::restore(
            &prefix[..partial_start],
            response_id,
            provider_sequence,
            expected_model,
            limits,
        )?
        else {
            return Ok(None);
        };
        let local_sequence = prefix
            .last()
            .map(EventEnvelope::sequence)
            .ok_or_else(|| error::malformed("persisted OpenAI prefix is empty"))?;
        Ok(Some(Self {
            local_sequence,
            provider_sequence,
            profile_id,
            profile_revision,
            expected_model: expected_model.clone(),
            state,
            partial: partial.to_vec(),
        }))
    }

    pub(crate) fn matches(
        &self,
        response_id: &ResponseId,
        provider_sequence: u64,
        profile_id: ProviderProfileId,
        profile_revision: u64,
        expected_model: &ModelName,
    ) -> bool {
        self.provider_sequence == provider_sequence
            && self.profile_id == profile_id
            && self.profile_revision == profile_revision
            && &self.expected_model == expected_model
            && self.state.response_id() == Some(response_id)
    }

    fn same_authority(&self, other: &Self) -> bool {
        self.profile_id == other.profile_id
            && self.profile_revision == other.profile_revision
            && self.expected_model == other.expected_model
            && self.state.response_id() == other.state.response_id()
    }
}

struct BackgroundResponseRecord {
    model: ModelName,
    checkpoint: Option<OpenAiResumeState>,
}

/// Shared active background-response ownership for creation, resumption, and cancellation.
#[derive(Clone, Default)]
pub(crate) struct BackgroundResponseRegistry {
    records: Arc<Mutex<BTreeMap<ResponseId, BackgroundResponseRecord>>>,
}

impl BackgroundResponseRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register_observed(
        &self,
        response_id: ResponseId,
        model: &ModelName,
    ) -> Result<(), ProviderCoreError> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| error::malformed("OpenAI background-response registry was unavailable"))?;
        if let Some(record) = records.get(&response_id) {
            if &record.model != model {
                return Err(error::malformed(
                    "OpenAI background response changed its model authority",
                ));
            }
            return Ok(());
        }
        records.insert(
            response_id,
            BackgroundResponseRecord { model: model.clone(), checkpoint: None },
        );
        Ok(())
    }

    pub(crate) fn restore_active(
        &self,
        response_id: ResponseId,
        model: &ModelName,
        checkpoint: Option<OpenAiResumeState>,
    ) -> Result<(), ProviderCoreError> {
        if checkpoint
            .as_ref()
            .is_some_and(|state| &state.expected_model != model)
        {
            return Err(error::invalid(
                "restored OpenAI response changed its model authority",
            ));
        }
        let mut records = self
            .records
            .lock()
            .map_err(|_| error::invalid("OpenAI background-response registry is unavailable"))?;
        if let Some(record) = records.get_mut(&response_id) {
            if &record.model != model {
                return Err(error::invalid(
                    "restored OpenAI response changed its model authority",
                ));
            }
            if let (Some(existing), Some(replacement)) =
                (record.checkpoint.as_ref(), checkpoint.as_ref())
                && !existing.same_authority(replacement)
            {
                return Err(error::invalid(
                    "restored OpenAI response changed its profile authority",
                ));
            }
            if checkpoint.is_some() {
                record.checkpoint = checkpoint;
            }
            return Ok(());
        }
        records.insert(
            response_id,
            BackgroundResponseRecord { model: model.clone(), checkpoint },
        );
        Ok(())
    }

    pub(crate) fn claim_exact(
        &self,
        response_id: &ResponseId,
        provider_sequence: u64,
        profile_id: ProviderProfileId,
        profile_revision: u64,
        expected_model: &ModelName,
    ) -> Result<OpenAiResumeState, ProviderCoreError> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| error::invalid("OpenAI background-response registry is unavailable"))?;
        let record = records.get_mut(response_id).ok_or_else(|| {
            error::invalid(
                "exact continuation is limited to active background responses owned by this adapter",
            )
        })?;
        if &record.model != expected_model
            || !record.checkpoint.as_ref().is_some_and(|state| {
                state.matches(
                    response_id,
                    provider_sequence,
                    profile_id,
                    profile_revision,
                    expected_model,
                )
            })
        {
            return Err(error::invalid(
                "exact continuation does not match a restored OpenAI decoder checkpoint",
            ));
        }
        record.checkpoint.take().ok_or_else(|| {
            error::invalid("OpenAI restored continuation checkpoint is unavailable")
        })
    }

    pub(crate) fn return_checkpoint(
        &self,
        response_id: &ResponseId,
        checkpoint: OpenAiResumeState,
    ) {
        let mut records = match self.records.lock() {
            Ok(records) => records,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(record) = records.get_mut(response_id)
            && record.model == checkpoint.expected_model
            && record.checkpoint.is_none()
        {
            record.checkpoint = Some(checkpoint);
        }
    }

    pub(crate) fn require_active(
        &self,
        response_id: &ResponseId,
    ) -> Result<(), ProviderCoreError> {
        let records = self
            .records
            .lock()
            .map_err(|_| error::invalid("OpenAI background-response registry is unavailable"))?;
        if !records.contains_key(response_id) {
            return Err(error::invalid(
                "provider cancellation requires an active background response owned by this adapter",
            ));
        }
        Ok(())
    }

    pub(crate) fn retire_terminal(
        &self,
        response_id: &ResponseId,
    ) -> Result<(), ProviderCoreError> {
        self.records
            .lock()
            .map_err(|_| error::malformed("OpenAI background-response registry was unavailable"))?
            .remove(response_id);
        Ok(())
    }

    pub(crate) fn retire_cancelled(
        &self,
        response_id: &ResponseId,
    ) -> Result<(), ProviderCoreError> {
        self.records
            .lock()
            .map_err(|_| error::invalid("OpenAI background-response registry is unavailable"))?
            .remove(response_id);
        Ok(())
    }
}

#[cfg(test)]
impl From<Arc<Mutex<BTreeSet<ResponseId>>>> for BackgroundResponseRegistry {
    fn from(_legacy: Arc<Mutex<BTreeSet<ResponseId>>>) -> Self {
        Self::new()
    }
}

impl OpenAiStream {
    #[allow(clippy::too_many_arguments, reason = "stream binds independent validated context")]
    pub(crate) fn new<R>(
        body: Box<dyn ByteStream>,
        framing_limits: FramingLimits,
        provider: ProviderName,
        expected_model: ModelName,
        structured_output: bool,
        limits: ProtocolLimits,
        metadata: metadata::ResponseMetadata,
        track_background: bool,
        background_responses: R,
    ) -> Self
    where
        R: Into<BackgroundResponseRegistry>,
    {
        Self {
            body,
            parser: SseParser::new(framing_limits),
            pending: VecDeque::new(),
            staged_terminal: None,
            local_sequence: 0,
            terminal: false,
            body_finished: false,
            provider,
            expected_model,
            structured_output,
            limits,
            state: state::ResponseState::new(),
            metadata,
            track_background,
            background_responses: background_responses.into(),
            restored_partial: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments, reason = "resume binds independent validated context")]
    pub(crate) fn resume(
        body: Box<dyn ByteStream>,
        framing_limits: FramingLimits,
        provider: ProviderName,
        structured_output: bool,
        limits: ProtocolLimits,
        metadata: metadata::ResponseMetadata,
        background_responses: BackgroundResponseRegistry,
        restored: OpenAiResumeState,
    ) -> Self {
        Self {
            body,
            parser: SseParser::new(framing_limits),
            pending: VecDeque::new(),
            staged_terminal: None,
            local_sequence: restored.local_sequence,
            terminal: false,
            body_finished: false,
            provider,
            expected_model: restored.expected_model,
            structured_output,
            limits,
            state: restored.state,
            metadata,
            track_background: true,
            background_responses,
            restored_partial: restored.partial,
        }
    }

    pub(crate) fn failure_stream(
        provider: ProviderName,
        event: ModelEvent,
        digest: peritus_types::Sha256Digest,
    ) -> Result<Self, ProviderCoreError> {
        let envelope = EventEnvelope::new(1, None, None, digest, event)
            .map_err(|_| error::malformed("failure envelope construction failed"))?;
        let limits = peritus_provider_core::HttpLimits::new([1, 1, 1, 1, 1])?;
        let body = peritus_provider_core::MemoryByteStream::new(Vec::new(), limits)?;
        Ok(Self {
            body: Box::new(body),
            parser: SseParser::new(FramingLimits::PRODUCTION),
            pending: VecDeque::new(),
            staged_terminal: Some(envelope),
            local_sequence: 1,
            terminal: true,
            body_finished: true,
            expected_model: ModelName::new("unknown".to_owned())
                .map_err(|_| error::malformed("static model identity was invalid"))?,
            structured_output: false,
            provider,
            limits: ProtocolLimits::PRODUCTION,
            state: state::ResponseState::new(),
            metadata: metadata::ResponseMetadata::empty(),
            track_background: false,
            background_responses: BackgroundResponseRegistry::new(),
            restored_partial: Vec::new(),
        })
    }

    fn process_items(&mut self, items: Vec<SseItem>) -> Result<(), ProviderCoreError> {
        for item in items {
            match item {
                SseItem::Event(frame) => self.decode_frame(&frame)?,
                SseItem::Comment(_) => {
                    self.enqueue(
                        None,
                        None,
                        peritus_codec::sha256(b"openai-sse-comment"),
                        ModelEvent::Heartbeat,
                    )?;
                }
                SseItem::Done if !self.terminal => {
                    self.fail_incomplete("OpenAI sent DONE before a response terminal")?;
                }
                SseItem::Done => {}
            }
        }
        Ok(())
    }

    fn enqueue(
        &mut self,
        provider_sequence: Option<u64>,
        provider_event_id: Option<peritus_model_protocol::EventId>,
        digest: peritus_types::Sha256Digest,
        event: ModelEvent,
    ) -> Result<(), ProviderCoreError> {
        let terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        if self.terminal && !terminal {
            return Err(error::malformed("OpenAI event followed a terminal event"));
        }
        let sequence = if terminal {
            if let Some(staged) = &self.staged_terminal {
                staged.sequence()
            } else {
                if self.terminal {
                    return Err(error::malformed("OpenAI terminal event was duplicated"));
                }
                self.local_sequence = self
                    .local_sequence
                    .checked_add(1)
                    .ok_or_else(|| error::limit("local event sequence overflowed"))?;
                self.local_sequence
            }
        } else {
            self.local_sequence = self
                .local_sequence
                .checked_add(1)
                .ok_or_else(|| error::limit("local event sequence overflowed"))?;
            self.local_sequence
        };
        let envelope = EventEnvelope::new(
            sequence,
            provider_sequence,
            provider_event_id,
            digest,
            event,
        )
        .map_err(|_| error::malformed("normalized OpenAI event was invalid"))?;
        if terminal {
            let replace_completion = self.staged_terminal.as_ref().is_some_and(|staged| {
                matches!(staged.event(), ModelEvent::ResponseCompleted)
                    && !matches!(envelope.event(), ModelEvent::ResponseCompleted)
            });
            let duplicate_completion = self.staged_terminal.as_ref().is_some_and(|staged| {
                matches!(staged.event(), ModelEvent::ResponseCompleted)
                    && matches!(envelope.event(), ModelEvent::ResponseCompleted)
            });
            if duplicate_completion {
                return Err(error::malformed("OpenAI completed more than once"));
            }
            if self.staged_terminal.is_none() || replace_completion {
                self.staged_terminal = Some(envelope);
            }
            self.terminal = true;
        } else {
            self.pending.push_back(envelope);
        }
        Ok(())
    }

    fn can_accept_cancellation(&self) -> bool {
        !self.terminal
            || self.staged_terminal.as_ref().is_some_and(|envelope| {
                matches!(envelope.event(), ModelEvent::ResponseCompleted)
            })
    }

    fn cancel_unpublished(&mut self) -> Result<bool, ProviderCoreError> {
        if !self.can_accept_cancellation() {
            return Ok(false);
        }
        self.body_finished = true;
        self.enqueue(
            None,
            None,
            peritus_codec::sha256(b"openai-local-cancellation"),
            ModelEvent::ResponseCancelled,
        )?;
        Ok(true)
    }

    fn fail_incomplete(&mut self, code: &'static str) -> Result<(), ProviderCoreError> {
        let failure = error::failure(
            &self.provider,
            FailureCategory::IncompleteStream,
            if self.local_sequence == 0 {
                TransportPhase::ReadingBody
            } else {
                TransportPhase::StreamObserved
            },
            if self.local_sequence == 0 {
                OutcomeCertainty::MaybeAccepted
            } else {
                OutcomeCertainty::AcceptedPartial
            },
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            code,
        )?;
        self.enqueue(
            None,
            None,
            peritus_codec::sha256(code.as_bytes()),
            ModelEvent::ResponseFailed(failure),
        )
    }

    fn fail_malformed(
        &mut self,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        let failure = error::failure(
            &self.provider,
            FailureCategory::MalformedPayload,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            "openai.stream.malformed",
        )?;
        self.enqueue(None, None, digest, ModelEvent::ResponseFailed(failure))
    }

    fn transport_terminal(&mut self, failure: &ProviderCoreError) -> Result<(), ProviderCoreError> {
        if failure.kind() == ProviderCoreErrorKind::Cancelled {
            return self.enqueue(
                None,
                None,
                peritus_codec::sha256(b"openai-local-cancellation"),
                ModelEvent::ResponseCancelled,
            );
        }
        let normalized = error::failure(
            &self.provider,
            FailureCategory::Transport,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            Some(200),
            self.state.response_id().cloned(),
            None,
            "openai.stream.interrupted",
        )?;
        self.enqueue(
            None,
            None,
            peritus_codec::sha256(b"openai-stream-interrupted"),
            ModelEvent::ResponseFailed(normalized),
        )
    }
}

impl ModelStream for OpenAiStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            loop {
                if cancellation.is_cancelled() {
                    let _ = self.cancel_unpublished()?;
                }
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                if let Some(event) = self.staged_terminal.take() {
                    self.body_finished = true;
                    return Ok(Some(event));
                }
                if self.terminal || self.body_finished {
                    return Ok(None);
                }
                match self.body.next(cancellation).await {
                    Ok(Some(chunk)) => match self.parser.push(&chunk) {
                        Ok(items) => {
                            if let Err(_failure) = self.process_items(items) {
                                self.fail_malformed(peritus_codec::sha256(&chunk))?;
                            }
                        }
                        Err(_failure) => self.fail_malformed(peritus_codec::sha256(&chunk))?,
                    },
                    Ok(None) => {
                        self.body_finished = true;
                        match self.parser.finish() {
                            Ok(items) => {
                                if self.process_items(items).is_err() {
                                    self.fail_malformed(peritus_codec::sha256(
                                        b"openai-final-frame",
                                    ))?;
                                }
                            }
                            Err(_failure) => {
                                self.fail_malformed(peritus_codec::sha256(b"openai-final-frame"))?;
                            }
                        }
                        if !self.terminal {
                            self.fail_incomplete("openai.stream.incomplete")?;
                        }
                    }
                    Err(failure) => {
                        self.body_finished = true;
                        self.transport_terminal(&failure)?;
                    }
                }
            }
        })
    }
}

impl fmt::Debug for OpenAiStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiStream")
            .field("local_sequence", &self.local_sequence)
            .field("pending_events", &self.pending.len())
            .field("staged_terminal", &self.staged_terminal.is_some())
            .field("terminal", &self.terminal)
            .field("body_finished", &self.body_finished)
            .field("body", &"[private byte stream]")
            .finish_non_exhaustive()
    }
}
