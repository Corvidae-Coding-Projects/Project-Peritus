//! Bounded SSE ownership and normalized compatible event emission.

mod ancillary;
mod chat;
mod responses;
mod responses_state;

use core::fmt;
use std::collections::VecDeque;

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ModelName, OutcomeCertainty, ProtocolLimits,
    ProviderName, ResponseId, Retryability, TransportPhase, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, FramingLimits, ModelStream, ProviderCoreError,
    ProviderCoreErrorKind, SseItem, SseParser,
};

use crate::error;

pub struct CompatibleStream {
    body: Box<dyn ByteStream>,
    parser: SseParser,
    pending: VecDeque<EventEnvelope>,
    sequence: u64,
    terminal: bool,
    body_finished: bool,
    provider: ProviderName,
    decoder: Decoder,
    metadata: VecDeque<ModelEvent>,
}

enum Decoder {
    Responses(responses::ResponsesDecoder),
    Chat(chat::ChatDecoder),
}

impl Decoder {
    const fn response_id(&self) -> Option<&ResponseId> {
        match self {
            Self::Responses(value) => value.response_id(),
            Self::Chat(value) => value.response_id(),
        }
    }

    fn decode(
        &mut self,
        frame: &peritus_provider_core::SseFrame,
    ) -> Result<responses::FrameEvents, ProviderCoreError> {
        match self {
            Self::Responses(value) => value.decode(frame),
            Self::Chat(value) => value.decode(frame),
        }
    }

    fn done(&self) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        match self {
            Self::Responses(_) => responses::ResponsesDecoder::done(),
            Self::Chat(value) => value.done(),
        }
    }
}

impl CompatibleStream {
    #[allow(clippy::too_many_arguments, reason = "stream binds independent validated context")]
    pub(crate) fn new(
        body: Box<dyn ByteStream>,
        framing: FramingLimits,
        provider: ProviderName,
        model: ModelName,
        dialect: WireDialect,
        structured: bool,
        allow_tools: bool,
        allow_usage: bool,
        limits: ProtocolLimits,
        metadata: Vec<ModelEvent>,
    ) -> Result<Self, ProviderCoreError> {
        let decoder = match dialect {
            WireDialect::CompatibleResponses => {
                Decoder::Responses(responses::ResponsesDecoder::new(
                    provider.clone(),
                    model,
                    structured,
                    allow_tools,
                    allow_usage,
                    limits,
                ))
            }
            WireDialect::CompatibleChatCompletions => Decoder::Chat(chat::ChatDecoder::new(
                provider.clone(),
                model,
                structured,
                allow_tools,
                allow_usage,
                limits,
            )),
            _ => return Err(error::configuration("compatible stream dialect changed")),
        };
        Ok(Self {
            body,
            parser: SseParser::new(framing),
            pending: VecDeque::new(),
            sequence: 0,
            terminal: false,
            body_finished: false,
            provider,
            decoder,
            metadata: VecDeque::from(metadata),
        })
    }

    pub(crate) fn failure_stream(
        provider: ProviderName,
        event: ModelEvent,
        digest: peritus_types::Sha256Digest,
    ) -> Result<Self, ProviderCoreError> {
        let envelope = EventEnvelope::new(1, None, None, digest, event)
            .map_err(|_| error::malformed("compatible failure envelope was invalid"))?;
        let memory_limits = peritus_provider_core::HttpLimits::new([1, 1, 1, 1, 1])?;
        let body = peritus_provider_core::MemoryByteStream::new(Vec::new(), memory_limits)?;
        let provider_copy = provider.clone();
        let model = ModelName::new("unknown".to_owned())
            .map_err(|_| error::malformed("static compatible model was invalid"))?;
        Ok(Self {
            body: Box::new(body),
            parser: SseParser::new(FramingLimits::PRODUCTION),
            pending: VecDeque::from([envelope]),
            sequence: 1,
            terminal: true,
            body_finished: true,
            provider,
            decoder: Decoder::Responses(responses::ResponsesDecoder::new(
                provider_copy,
                model,
                false,
                false,
                false,
                ProtocolLimits::PRODUCTION,
            )),
            metadata: VecDeque::new(),
        })
    }

    fn process(&mut self, items: Vec<SseItem>) -> Result<(), ProviderCoreError> {
        for item in items {
            match item {
                SseItem::Event(frame) => {
                    if self.terminal {
                        return Err(error::malformed("compatible data followed a terminal event"));
                    }
                    let decoded = self.decoder.decode(&frame)?;
                    for (index, event) in decoded.events.into_iter().enumerate() {
                        let started = matches!(event, ModelEvent::ResponseStarted { .. });
                        self.enqueue(
                            (index == 0).then_some(decoded.provider_sequence).flatten(),
                            (index == 0).then(|| decoded.provider_event_id.clone()).flatten(),
                            decoded.digest,
                            event,
                        )?;
                        if started {
                            while let Some(event) = self.metadata.pop_front() {
                                self.enqueue(
                                    None,
                                    None,
                                    peritus_codec::sha256(b"compatible-response-metadata"),
                                    event,
                                )?;
                            }
                        }
                    }
                }
                SseItem::Comment(_) => self.enqueue(
                    None,
                    None,
                    peritus_codec::sha256(b"compatible-sse-comment"),
                    ModelEvent::Heartbeat,
                )?,
                SseItem::Done if self.terminal => {}
                SseItem::Done => {
                    let events = self.decoder.done()?;
                    for event in events {
                        self.enqueue(
                            None,
                            None,
                            peritus_codec::sha256(b"compatible-sse-done"),
                            event,
                        )?;
                    }
                }
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
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| error::limit("compatible local event sequence overflowed"))?;
        let terminal = matches!(
            event,
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        );
        let envelope =
            EventEnvelope::new(self.sequence, provider_sequence, provider_event_id, digest, event)
                .map_err(|_| error::malformed("compatible normalized event was invalid"))?;
        self.pending.push_back(envelope);
        self.terminal |= terminal;
        Ok(())
    }

    fn fail(
        &mut self,
        category: FailureCategory,
        code: &'static str,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        let failure = error::failure(
            &self.provider,
            category,
            if self.sequence == 0 {
                TransportPhase::ReadingBody
            } else {
                TransportPhase::StreamObserved
            },
            if self.sequence == 0 {
                OutcomeCertainty::MaybeAccepted
            } else {
                OutcomeCertainty::AcceptedPartial
            },
            Retryability::Never,
            Some(200),
            self.decoder.response_id().cloned(),
            None,
            code,
        )?;
        self.enqueue(None, None, digest, ModelEvent::ResponseFailed(failure))
    }

    fn transport_terminal(&mut self, failure: &ProviderCoreError) -> Result<(), ProviderCoreError> {
        if failure.kind() == ProviderCoreErrorKind::Cancelled {
            return self.enqueue(
                None,
                None,
                peritus_codec::sha256(b"compatible-local-cancel"),
                ModelEvent::ResponseCancelled,
            );
        }
        self.fail(
            FailureCategory::Transport,
            "compatible.stream.interrupted",
            peritus_codec::sha256(b"compatible-stream-interrupted"),
        )
    }
}

impl ModelStream for CompatibleStream {
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
                    Ok(Some(chunk)) => {
                        let invalid = self
                            .parser
                            .push(&chunk)
                            .map_or(true, |items| self.process(items).is_err());
                        if invalid {
                            self.fail(
                                FailureCategory::MalformedPayload,
                                "compatible.stream.malformed",
                                peritus_codec::sha256(&chunk),
                            )?;
                        }
                    }
                    Ok(None) => {
                        self.body_finished = true;
                        let invalid =
                            self.parser.finish().map_or(true, |items| self.process(items).is_err());
                        if invalid {
                            self.fail(
                                FailureCategory::MalformedPayload,
                                "compatible.stream.malformed",
                                peritus_codec::sha256(b"compatible-final-frame"),
                            )?;
                        }
                        if !self.terminal {
                            self.fail(
                                FailureCategory::IncompleteStream,
                                "compatible.stream.incomplete",
                                peritus_codec::sha256(b"compatible-stream-incomplete"),
                            )?;
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

impl fmt::Debug for CompatibleStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleStream")
            .field("sequence", &self.sequence)
            .field("pending_events", &self.pending.len())
            .field("terminal", &self.terminal)
            .field("body_finished", &self.body_finished)
            .field("body", &"[private byte stream]")
            .finish_non_exhaustive()
    }
}
