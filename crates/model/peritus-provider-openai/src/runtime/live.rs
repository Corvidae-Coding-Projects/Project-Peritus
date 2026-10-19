//! Pull-driven execution with public Codex summaries observed before process completion.
//! Structured messages/tool calls remain withheld until the existing final validation succeeds.

use peritus_model_protocol::{
    EventEnvelope, ItemId, ItemKind, ModelEvent, ModelName, ProtocolLimits, StreamFragment,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelStream, OwnedModelStream, ProviderCoreError,
};
use std::{
    collections::VecDeque,
    future::poll_fn,
    sync::{Arc, Mutex},
    task::Poll,
};

#[derive(Default)]
pub(super) struct Progress(Mutex<Observed>);

#[derive(Default)]
struct Observed {
    offset: usize,
    summaries: u32,
    pending: VecDeque<ModelEvent>,
}

impl Progress {
    /// A rejected pre-inference handshake can safely choose the ordinary transport.
    /// Its byte stream starts at zero; already published item identities stay monotonic.
    pub(super) fn begin_transport(&self) {
        if let Ok(mut state) = self.0.lock() {
            state.offset = 0;
        }
    }

    pub(super) fn observe(&self, bytes: &[u8]) {
        let Ok(mut state) = self.0.lock() else { return };
        while let Some(end) =
            bytes.get(state.offset..).and_then(|tail| tail.iter().position(|byte| *byte == b'\n'))
        {
            let start = state.offset;
            state.offset += end + 1;
            let Ok(value) =
                serde_json::from_slice::<serde_json::Value>(&bytes[start..state.offset])
            else {
                continue;
            };
            if value["type"] != "item.completed" || value["item"]["type"] != "reasoning" {
                continue;
            }
            let Some(text) = value["item"]["text"].as_str().filter(|text| !text.is_empty()) else {
                continue;
            };
            if text.len() > ProtocolLimits::PRODUCTION.max_text_bytes() {
                continue;
            }
            let Ok(id) = ItemId::new(format!("codex-public-summary-{}", state.summaries)) else {
                continue;
            };
            let index = state.summaries;
            state.summaries = state.summaries.saturating_add(1);
            state.pending.push_back(ModelEvent::ItemStarted {
                item_id: id.clone(),
                index,
                kind: ItemKind::Reasoning,
            });
            for chunk in text.as_bytes().chunks(ProtocolLimits::PRODUCTION.max_event_bytes()) {
                if let Ok(fragment) =
                    StreamFragment::new(chunk.to_vec(), ProtocolLimits::PRODUCTION)
                {
                    state.pending.push_back(ModelEvent::ReasoningSummaryDelta {
                        item_id: id.clone(),
                        fragment,
                    });
                }
            }
            state.pending.push_back(ModelEvent::ItemCompleted(id));
        }
    }
}

pub(super) struct LiveRuntimeStream {
    operation: Option<BoxFuture<'static, Result<OwnedModelStream, ProviderCoreError>>>,
    completed: Option<OwnedModelStream>,
    progress: Arc<Progress>,
    sequence: u64,
    model: ModelName,
    started: bool,
}

impl LiveRuntimeStream {
    pub(super) fn new(
        model: ModelName,
        operation: BoxFuture<'static, Result<OwnedModelStream, ProviderCoreError>>,
        progress: Arc<Progress>,
    ) -> Self {
        Self {
            operation: Some(operation),
            completed: None,
            progress,
            sequence: 0,
            model,
            started: false,
        }
    }

    fn envelope(&mut self, event: ModelEvent) -> Result<EventEnvelope, ProviderCoreError> {
        self.sequence = self.sequence.checked_add(1).ok_or_else(malformed)?;
        EventEnvelope::new(
            self.sequence,
            None,
            None,
            peritus_codec::sha256(b"codex-public-progress"),
            event,
        )
        .map_err(|_| malformed())
    }
}

impl ModelStream for LiveRuntimeStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            loop {
                if self.operation.is_some() {
                    let event = poll_fn(|context| {
                        if let Some(operation) = self.operation.as_mut()
                            && let Poll::Ready(result) = operation.as_mut().poll(context)
                        {
                            self.operation = None;
                            match result {
                                Ok(stream) => self.completed = Some(stream),
                                Err(error) => return Poll::Ready(Err(error)),
                            }
                        }
                        let mut observed = self.progress.0.lock().map_err(|_| malformed())?;
                        if !observed.pending.is_empty() && !self.started {
                            self.started = true;
                            return Poll::Ready(Ok(Some(ModelEvent::ResponseStarted {
                                response_id: None,
                                model: Some(self.model.clone()),
                            })));
                        }
                        if let Some(event) = observed.pending.pop_front() {
                            return Poll::Ready(Ok(Some(event)));
                        }
                        if self.operation.is_none() { Poll::Ready(Ok(None)) } else { Poll::Pending }
                    })
                    .await?;
                    if let Some(event) = event {
                        return self.envelope(event).map(Some);
                    }
                }
                // Drain observations from the last process poll before its validated result.
                let event = self.progress.0.lock().map_err(|_| malformed())?.pending.pop_front();
                if let Some(event) = event {
                    return self.envelope(event).map(Some);
                }
                let Some(completed) = &mut self.completed else { return Ok(None) };
                let Some(envelope) = completed.pull().await? else { return Ok(None) };
                let mut event = envelope.event().clone();
                if matches!(event, ModelEvent::ResponseStarted { .. }) {
                    if self.started {
                        continue;
                    }
                    self.started = true;
                }
                if let ModelEvent::ItemStarted { index, .. } = &mut event {
                    *index = index
                        .checked_add(self.progress.0.lock().map_err(|_| malformed())?.summaries)
                        .ok_or_else(malformed)?;
                }
                return self.envelope(event).map(Some);
            }
        })
    }
}

const fn malformed() -> ProviderCoreError {
    ProviderCoreError::malformed_stream(
        "codex_live_output",
        "Codex public progress could not be normalized",
    )
}
