//! Bounded `OpenAI` Responses SSE ownership and normalized event emission.

mod decode;
pub mod metadata;
mod output;
mod state;
mod terminal;

use core::fmt;
use std::collections::{BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ModelName, OutcomeCertainty, ProtocolLimits,
    ProviderName, ResponseId, Retryability, TransportPhase,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, FramingLimits, ModelStream, ProviderCoreError,
    ProviderCoreErrorKind, SseItem, SseParser,
};

use crate::error;

#[allow(
    clippy::struct_excessive_bools,
    reason = "decoder lifecycle and request properties are independent audited state"
)]
pub struct OpenAiStream {
    body: Box<dyn ByteStream>,
    parser: SseParser,
    pending: VecDeque<EventEnvelope>,
    local_sequence: u64,
    terminal: bool,
    body_finished: bool,
    provider: ProviderName,
    expected_model: ModelName,
    structured_output: bool,
    limits: ProtocolLimits,
    state: state::ResponseState,
    metadata: metadata::ResponseMetadata,
    register_background: bool,
    resumable: Arc<Mutex<BTreeSet<ResponseId>>>,
    restored_partial: Vec<EventEnvelope>,
}

/// Decoder checkpoint rebuilt from the exact durable normalized prefix.
pub(crate) struct OpenAiResumeState {
    local_sequence: u64,
    provider_sequence: u64,
    expected_model: ModelName,
    state: state::ResponseState,
    partial: Vec<EventEnvelope>,
}

impl OpenAiResumeState {
    pub(crate) fn restore(
        prefix: &[EventEnvelope],
        response_id: &ResponseId,
        provider_sequence: u64,
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
            expected_model: expected_model.clone(),
            state,
            partial: partial.to_vec(),
        }))
    }

    pub(crate) fn matches(
        &self,
        response_id: &ResponseId,
        provider_sequence: u64,
        expected_model: &ModelName,
    ) -> bool {
        self.provider_sequence == provider_sequence
            && &self.expected_model == expected_model
            && self.state.response_id() == Some(response_id)
    }
}

impl OpenAiStream {
    #[allow(clippy::too_many_arguments, reason = "stream binds independent validated context")]
    pub(crate) fn new(
        body: Box<dyn ByteStream>,
        framing_limits: FramingLimits,
        provider: ProviderName,
        expected_model: ModelName,
        structured_output: bool,
        limits: ProtocolLimits,
        metadata: metadata::ResponseMetadata,
        register_background: bool,
        resumable: Arc<Mutex<BTreeSet<ResponseId>>>,
    ) -> Self {
        Self {
            body,
            parser: SseParser::new(framing_limits),
            pending: VecDeque::new(),
            local_sequence: 0,
            terminal: false,
            body_finished: false,
            provider,
            expected_model,
            structured_output,
            limits,
            state: state::ResponseState::new(),
            metadata,
            register_background,
            resumable,
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
        resumable: Arc<Mutex<BTreeSet<ResponseId>>>,
        restored: OpenAiResumeState,
    ) -> Self {
        Self {
            body,
            parser: SseParser::new(framing_limits),
            pending: VecDeque::new(),
            local_sequence: restored.local_sequence,
            terminal: false,
            body_finished: false,
            provider,
            expected_model: restored.expected_model,
            structured_output,
            limits,
            state: restored.state,
            metadata,
            register_background: false,
            resumable,
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
            pending: VecDeque::from([envelope]),
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
            register_background: false,
            resumable: Arc::new(Mutex::new(BTreeSet::new())),
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
        self.local_sequence = self
            .local_sequence
            .checked_add(1)
            .ok_or_else(|| error::limit("local event sequence overflowed"))?;
        let terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        let envelope = EventEnvelope::new(
            self.local_sequence,
            provider_sequence,
            provider_event_id,
            digest,
            event,
        )
        .map_err(|_| error::malformed("normalized OpenAI event was invalid"))?;
        self.pending.push_back(envelope);
        self.terminal |= terminal;
        Ok(())
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
                if let Some(event) = self.pending.pop_front() {
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
            .field("terminal", &self.terminal)
            .field("body_finished", &self.body_finished)
            .field("body", &"[private byte stream]")
            .finish_non_exhaustive()
    }
}
