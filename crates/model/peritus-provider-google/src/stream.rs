//! Cancellation-aware pull stream over bounded Google stable-v1 SSE framing.

mod generate;
mod interaction_fields;
mod interactions;
mod state;
mod value;

use core::fmt;
use std::collections::VecDeque;

use peritus_model_protocol::{
    EventEnvelope, FailureCategory, ModelEvent, ProtocolLimits, ProviderName, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, FramingLimits, HttpResponse, ModelStream,
    ProviderCoreError, ProviderCoreErrorKind, SseItem, SseParser,
};

use crate::error::stream_failure;
use state::NormalizeState;

pub struct GoogleStream {
    body: Option<Box<dyn ByteStream>>,
    parser: SseParser,
    state: NormalizeState,
    pending: VecDeque<EventEnvelope>,
    provider: ProviderName,
    ended: bool,
}

impl GoogleStream {
    pub(crate) fn new(
        response: HttpResponse,
        provider: ProviderName,
        dialect: WireDialect,
        structured: bool,
        tool_controls: crate::request::ToolControls,
        framing_limits: FramingLimits,
        protocol_limits: ProtocolLimits,
    ) -> Result<Self, ProviderCoreError> {
        let (_status, headers, body) = response.into_parts();
        Ok(Self {
            body: Some(body),
            parser: SseParser::new(framing_limits),
            state: NormalizeState::new(
                provider.clone(),
                dialect,
                structured,
                tool_controls,
                &headers,
                protocol_limits,
            )?,
            pending: VecDeque::new(),
            provider,
            ended: false,
        })
    }

    pub(crate) fn terminal(event: ModelEvent) -> Result<Self, ProviderCoreError> {
        let provider = ProviderName::new("google".to_owned()).map_err(|_| {
            ProviderCoreError::configuration(
                "google_stream",
                "static Google provider identity is invalid",
            )
        })?;
        let mut state = NormalizeState::new(
            provider.clone(),
            WireDialect::GeminiInteractionsV1,
            false,
            crate::request::ToolControls::terminal(),
            &peritus_provider_core::HttpHeaders::empty(),
            ProtocolLimits::PRODUCTION,
        )?;
        state.push_synthetic(event)?;
        Ok(Self {
            body: None,
            parser: SseParser::new(FramingLimits::PRODUCTION),
            state,
            pending: VecDeque::new(),
            provider,
            ended: false,
        })
    }

    fn drain_state(&mut self) {
        self.pending.extend(self.state.take_pending());
    }

    fn fail(
        &mut self,
        category: FailureCategory,
        code: &'static str,
    ) -> Result<(), ProviderCoreError> {
        let failure = stream_failure(
            self.provider.clone(),
            category,
            self.state.has_observed_semantics(),
            code,
        )?;
        self.state.push_synthetic(ModelEvent::ResponseFailed(failure))?;
        self.drain_state();
        Ok(())
    }

    fn process_items(&mut self, items: Vec<SseItem>) -> Result<(), ProviderCoreError> {
        for item in items {
            if self.state.process(item).is_err() {
                self.fail(FailureCategory::MalformedPayload, "google.stream.malformed")?;
                break;
            }
        }
        self.drain_state();
        Ok(())
    }
}

impl ModelStream for GoogleStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            loop {
                if self.ended {
                    return Ok(None);
                }
                if cancellation.is_cancelled() && self.state.cancel_unpublished()? {
                    self.pending.clear();
                    self.body = None;
                    self.drain_state();
                }
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                if let Some(event) = self.state.take_staged_terminal() {
                    self.ended = true;
                    self.body = None;
                    return Ok(Some(event));
                }
                let Some(body) = self.body.as_mut() else {
                    self.fail(
                        FailureCategory::IncompleteStream,
                        "google.stream.incomplete",
                    )?;
                    continue;
                };
                match body.next(cancellation).await {
                    Ok(Some(chunk)) => match self.parser.push(&chunk) {
                        Ok(items) => self.process_items(items)?,
                        Err(_error) => {
                            self.fail(FailureCategory::MalformedPayload, "google.sse.framing")?;
                        }
                    },
                    Ok(None) => {
                        match self.parser.finish() {
                            Ok(items) => self.process_items(items)?,
                            Err(_error) => self.fail(
                                FailureCategory::MalformedPayload,
                                "google.sse.incomplete_frame",
                            )?,
                        }
                        if !self.state.is_terminal() {
                            self.fail(
                                FailureCategory::IncompleteStream,
                                "google.stream.incomplete",
                            )?;
                        }
                        self.body = None;
                    }
                    Err(error) if error.kind() == ProviderCoreErrorKind::Cancelled => {
                        let _ = self.state.cancel_unpublished()?;
                        self.drain_state();
                        self.body = None;
                    }
                    Err(_error) => {
                        self.fail(FailureCategory::Transport, "google.stream.interrupted")?;
                        self.body = None;
                    }
                }
            }
        })
    }
}

impl fmt::Debug for GoogleStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleStream")
            .field("body", &self.body.as_ref().map(|_| "[private byte stream]"))
            .field("pending_events", &self.pending.len())
            .field("staged_terminal", &self.state.has_staged_terminal())
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
