//! Bounded SSE ownership and normalized compatible event emission.

mod ancillary;
mod chat;
mod identity;
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
    framed: VecDeque<FramedItem>,
    deferred: Option<DeferredFrame>,
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

struct FramedItem {
    item: SseItem,
    digest: peritus_types::Sha256Digest,
}

struct DeferredFrame {
    events: chat::ReasoningCompletion,
    digest: peritus_types::Sha256Digest,
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
            framed: VecDeque::new(),
            deferred: None,
            sequence: 0,
            terminal: false,
            body_finished: false,
            provider,
            decoder,
            metadata: VecDeque::from(metadata),
        })
    }

    pub(crate) const fn with_hosted_service(
        mut self,
        service: Option<peritus_provider_core::hosted::HostedService>,
    ) -> Self {
        if let Decoder::Chat(decoder) = &mut self.decoder {
            decoder.service = service;
        }
        self
    }

    pub(crate) fn with_raw_frame_capacity(
        mut self,
        maximum: usize,
    ) -> Result<Self, ProviderCoreError> {
        if matches!(&self.decoder, Decoder::Responses(_)) {
            self.parser.set_data_frame_capacity(maximum)?;
        }
        Ok(self)
    }

    pub(crate) fn with_tool_choice(mut self, choice: peritus_model_protocol::ToolChoice) -> Self {
        if let Decoder::Chat(decoder) = &mut self.decoder
            && decoder.service.is_some()
        {
            decoder.tool_choice = choice;
        }
        self
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
            framed: VecDeque::new(),
            deferred: None,
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

    fn process(
        &mut self,
        items: Vec<SseItem>,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        self.framed.extend(items.into_iter().map(|item| FramedItem { item, digest }));
        self.resume_framed()
    }

    fn resume_framed(&mut self) -> Result<(), ProviderCoreError> {
        while self.deferred.is_none() {
            let Some(framed) = self.framed.pop_front() else { break };
            if let Err(failure) = self.process_item(framed.item) {
                self.framed.clear();
                self.mapping_failure(&failure, framed.digest)?;
                break;
            }
        }
        Ok(())
    }

    fn process_item(&mut self, item: SseItem) -> Result<(), ProviderCoreError> {
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
                if let Some(events) = decoded.continuation {
                    if self.deferred.is_some() {
                        return Err(error::malformed("compatible deferred frame overlapped"));
                    }
                    self.deferred = Some(DeferredFrame { events, digest: decoded.digest });
                }
                Ok(())
            }
            SseItem::Comment(_) => self.enqueue(
                None,
                None,
                peritus_codec::sha256(b"compatible-sse-comment"),
                ModelEvent::Heartbeat,
            ),
            SseItem::Done if self.terminal => Ok(()),
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
                Ok(())
            }
        }
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
        retryability: Retryability,
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
            retryability,
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
            Retryability::Never,
        )
    }

    fn process_parsed(
        &mut self,
        parsed: Result<Vec<SseItem>, ProviderCoreError>,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        match parsed {
            Ok(items) => self.process(items, digest),
            Err(failure) => self.mapping_failure(&failure, digest),
        }
    }

    fn mapping_failure(
        &mut self,
        failure: &ProviderCoreError,
        digest: peritus_types::Sha256Digest,
    ) -> Result<(), ProviderCoreError> {
        let (category, code, retryability) = if chat::required_tool_choice_missing(failure) {
            (
                FailureCategory::Provider,
                "compatible.stream.required_tool_choice_missing",
                Retryability::SafeNewRequest,
            )
        } else {
            (
                FailureCategory::MalformedPayload,
                "compatible.stream.malformed",
                Retryability::Never,
            )
        };
        self.fail(category, code, digest, retryability)
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
                if self.deferred.is_some() {
                    if cancellation.is_cancelled()
                        && self
                            .deferred
                            .as_ref()
                            .is_some_and(|deferred| deferred.events.replaying())
                    {
                        self.deferred = None;
                        self.framed.clear();
                        self.body_finished = true;
                        self.enqueue(
                            None,
                            None,
                            peritus_codec::sha256(b"compatible-local-cancel"),
                            ModelEvent::ResponseCancelled,
                        )?;
                        continue;
                    }
                    let digest = self
                        .deferred
                        .as_ref()
                        .map(|deferred| deferred.digest)
                        .ok_or_else(|| error::malformed("compatible deferred frame disappeared"))?;
                    let next = self
                        .deferred
                        .as_mut()
                        .ok_or_else(|| error::malformed("compatible deferred frame disappeared"))?
                        .events
                        .next_event();
                    match next {
                        Ok(Some(event)) => {
                            self.enqueue(None, None, digest, event)?;
                            continue;
                        }
                        Ok(None) => {
                            self.deferred = None;
                            self.resume_framed()?;
                            continue;
                        }
                        Err(failure) => {
                            self.deferred = None;
                            self.framed.clear();
                            self.mapping_failure(&failure, digest)?;
                            continue;
                        }
                    }
                }
                self.resume_framed()?;
                if !self.pending.is_empty() || self.deferred.is_some() {
                    continue;
                }
                if self.terminal {
                    return Ok(None);
                }
                if self.body_finished {
                    self.fail(
                        FailureCategory::IncompleteStream,
                        "compatible.stream.incomplete",
                        peritus_codec::sha256(b"compatible-stream-incomplete"),
                        Retryability::Never,
                    )?;
                    continue;
                }
                match self.body.next(cancellation).await {
                    Ok(Some(chunk)) => {
                        let parsed = self.parser.push(&chunk);
                        self.process_parsed(parsed, peritus_codec::sha256(&chunk))?;
                    }
                    Ok(None) => {
                        self.body_finished = true;
                        let parsed = self.parser.finish();
                        self.process_parsed(
                            parsed,
                            peritus_codec::sha256(b"compatible-final-frame"),
                        )?;
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
            .field("pending_frames", &self.framed.len())
            .field("deferred", &self.deferred.is_some())
            .field("terminal", &self.terminal)
            .field("body_finished", &self.body_finished)
            .field("body", &"[private byte stream]")
            .finish_non_exhaustive()
    }
}
