//! Cancellation-aware pull stream over bounded Anthropic SSE framing.

mod content;
mod message;
mod state;
mod value;

use core::fmt;
use std::collections::VecDeque;

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ProtocolLimits, ProviderName,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, FramingLimits, HttpResponse, ModelStream,
    ProviderCoreError, ProviderCoreErrorKind, SseItem, SseParser,
};

use crate::error::stream_failure;
use state::NormalizeState;

pub struct AnthropicStream {
    body: Option<Box<dyn ByteStream>>,
    parser: SseParser,
    state: NormalizeState,
    pending: VecDeque<EventEnvelope>,
    framed: VecDeque<SseItem>,
    provider: ProviderName,
    ended: bool,
}

impl AnthropicStream {
    #[cfg(test)]
    pub(crate) fn new(
        response: HttpResponse,
        provider: ProviderName,
        framing_limits: FramingLimits,
    ) -> Result<Self, ProviderCoreError> {
        Self::with_limits(
            response,
            provider,
            framing_limits,
            ProtocolLimits::PRODUCTION,
            peritus_provider_core::HttpLimits::PRODUCTION.max_response_body_bytes(),
        )
    }

    pub(crate) fn with_limits(
        response: HttpResponse,
        provider: ProviderName,
        framing_limits: FramingLimits,
        protocol_limits: ProtocolLimits,
        replay_identity_bytes: usize,
    ) -> Result<Self, ProviderCoreError> {
        let (_status, headers, body) = response.into_parts();
        Ok(Self {
            body: Some(body),
            parser: SseParser::new(framing_limits),
            state: NormalizeState::with_limits(
                provider.clone(),
                &headers,
                protocol_limits,
                replay_identity_bytes,
            )?,
            pending: VecDeque::new(),
            framed: VecDeque::new(),
            provider,
            ended: false,
        })
    }

    pub(crate) fn terminal(
        event: ModelEvent,
        protocol_limits: ProtocolLimits,
    ) -> Result<Self, ProviderCoreError> {
        let provider = ProviderName::new("anthropic".to_owned()).map_err(|_| {
            ProviderCoreError::configuration(
                "anthropic_stream",
                "static Anthropic provider identity is invalid",
            )
        })?;
        let mut state = NormalizeState::with_limits(
            provider.clone(),
            &peritus_provider_core::HttpHeaders::empty(),
            protocol_limits,
            1,
        )?;
        state.push_synthetic(event)?;
        Ok(Self {
            body: None,
            parser: SseParser::new(FramingLimits::PRODUCTION),
            state,
            pending: VecDeque::new(),
            framed: VecDeque::new(),
            provider,
            ended: false,
        })
    }

    fn drain_state(&mut self) {
        self.pending.extend(self.state.take_pending());
        if self.state.is_terminal() {
            self.ended = true;
        }
    }

    fn fail(
        &mut self,
        category: FailureCategory,
        code: &'static str,
    ) -> Result<(), ProviderCoreError> {
        self.framed.clear();
        self.state.clear_deferred_replay();
        if !self.state.is_terminal() {
            let failure = stream_failure(
                self.provider.clone(),
                category,
                self.state.has_observed_semantics(),
                code,
            )?;
            self.state.push_synthetic(ModelEvent::ResponseFailed(failure))?;
        }
        self.drain_state();
        Ok(())
    }

    fn process_items(&mut self, items: Vec<SseItem>) -> Result<(), ProviderCoreError> {
        if self.framed.try_reserve(items.len()).is_err() {
            self.fail(FailureCategory::MalformedPayload, "anthropic.stream.capacity")?;
            return Ok(());
        }
        self.framed.extend(items);
        self.resume_framed()
    }

    fn resume_framed(&mut self) -> Result<(), ProviderCoreError> {
        while !self.state.has_deferred_replay() {
            let Some(item) = self.framed.pop_front() else { break };
            if let Err(_error) = self.state.process(item) {
                self.fail(FailureCategory::MalformedPayload, "anthropic.stream.malformed")?;
                break;
            }
        }
        self.drain_state();
        Ok(())
    }
}

impl ModelStream for AnthropicStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            loop {
                if cancellation.is_cancelled() && !self.state.is_terminal() {
                    self.pending.clear();
                    self.framed.clear();
                    self.state.clear_deferred_replay();
                    self.body = None;
                    self.state.push_synthetic(ModelEvent::ResponseCancelled)?;
                    self.drain_state();
                }
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                if self.ended {
                    return Ok(None);
                }
                if self.state.has_deferred_replay() {
                    match self.state.emit_deferred_replay() {
                        Ok(true) => self.drain_state(),
                        Ok(false) => self.resume_framed()?,
                        Err(_error) => self.fail(
                            FailureCategory::MalformedPayload,
                            "anthropic.stream.replay",
                        )?,
                    }
                    continue;
                }
                self.resume_framed()?;
                if !self.pending.is_empty()
                    || self.state.has_deferred_replay()
                    || self.ended
                {
                    continue;
                }
                let Some(body) = self.body.as_mut() else {
                    self.drain_state();
                    return Ok(self.pending.pop_front());
                };
                match body.next(cancellation).await {
                    Ok(Some(chunk)) => match self.parser.push(&chunk) {
                        Ok(items) => self.process_items(items)?,
                        Err(_error) => {
                            self.fail(FailureCategory::MalformedPayload, "anthropic.sse.framing")?;
                        }
                    },
                    Ok(None) => {
                        match self.parser.finish() {
                            Ok(items) => self.process_items(items)?,
                            Err(_error) => self.fail(
                                FailureCategory::MalformedPayload,
                                "anthropic.sse.incomplete_frame",
                            )?,
                        }
                        if !self.state.is_terminal() {
                            self.fail(
                                FailureCategory::IncompleteStream,
                                "anthropic.stream.incomplete",
                            )?;
                        }
                        self.body = None;
                    }
                    Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                        self.state.push_synthetic(ModelEvent::ResponseCancelled)?;
                        self.drain_state();
                    }
                    Err(_error) => {
                        self.fail(FailureCategory::Transport, "anthropic.stream.interrupted")?;
                        self.body = None;
                    }
                }
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                if self.ended {
                    return Ok(None);
                }
            }
        })
    }
}

impl fmt::Debug for AnthropicStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnthropicStream")
            .field("body", &self.body.as_ref().map(|_| "[private byte stream]"))
            .field("pending_events", &self.pending.len())
            .field("framed_items", &self.framed.len())
            .field("deferred_replay", &self.state.has_deferred_replay())
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
