//! An owned connection attempt is polled beside input, never in front of the UI loop.

use super::{CLOSE_GRACE, TuiConfig, process_seed, protocol_id};
use crate::{
    TuiError,
    action::{Action, Effect},
    client::{ClientEvent, ClientSession},
    model::AppModel,
};
use std::{future::Future, pin::Pin};
use tokio::sync::mpsc;

mod outbox;

type Attempt = Pin<Box<dyn Future<Output = Result<ClientSession, TuiError>> + Send>>;

pub(super) struct Connection {
    pub(super) session: Option<ClientSession>,
    attempt: Option<Attempt>,
    generation: u64,
    events: mpsc::Sender<ClientEvent>,
    outbox: outbox::Outbox,
}

impl Connection {
    pub(super) fn new(events: mpsc::Sender<ClientEvent>) -> Self {
        Self {
            session: None,
            attempt: None,
            generation: 0,
            events,
            outbox: outbox::Outbox::default(),
        }
    }

    pub(super) fn start(&mut self, config: &TuiConfig, model: &mut AppModel) {
        self.outbox = outbox::Outbox::default();
        self.generation = self.generation.saturating_add(1);
        let protocol = protocol_id(process_seed(config.endpoint()), self.generation);
        let endpoint = config.endpoint().to_owned();
        let requested = model.retained_session().or_else(|| config.requested_session());
        let timeout = config.connection_timeout();
        let previous = self.session.take();
        let cleanup = model.cleanup_messages();
        model.update(Action::Connecting);
        let events = self.events.clone();
        self.attempt = Some(Box::pin(async move {
            if let Some(previous) = previous {
                // The prior connection still owns its bounded cleanup. Dropping this
                // attempt cancels cleanup and drops its reader/writer owner as well.
                let _ = previous.close(cleanup).await;
            }
            Self::connect_with_policy(&endpoint, protocol?, requested, timeout, events).await
        }));
    }

    pub(super) const fn active(&self) -> bool {
        self.attempt.is_some()
    }

    pub(super) async fn next(&mut self, model: &mut AppModel) -> Vec<Effect> {
        if let Some(attempt) = self.attempt.as_mut() {
            let result = attempt.await;
            self.attempt = None;
            self.finish(result, model)
        } else {
            let result = self.outbox.next().await;
            self.sent(result, model)
        }
    }

    pub(super) fn finish(
        &mut self,
        result: Result<ClientSession, TuiError>,
        model: &mut AppModel,
    ) -> Vec<Effect> {
        match result {
            Ok(session) => {
                let established = session.established().clone();
                self.session = Some(session);
                let mut effects = model.update(Action::Connected {
                    context: established.context,
                    limits: established.limits,
                    server: established.server,
                    downgraded: established.downgraded,
                });
                effects.extend(model.update(Action::NegotiatedFeatures {
                    context: established.context,
                    features: established.features,
                }));
                effects
            }
            Err(error) => model.update(Action::ConnectionFailed(error.to_string())),
        }
    }

    pub(super) fn cancel(&mut self) {
        self.attempt = None;
        self.outbox = outbox::Outbox::default();
    }

    pub(super) async fn close(
        &mut self,
        cleanup: Vec<peritus_app_protocol::AppMessage>,
    ) -> Result<(), TuiError> {
        self.attempt = None;
        let Some(session) = self.session.take() else {
            self.cancel();
            return Ok(());
        };
        let result = tokio::time::timeout(CLOSE_GRACE, async {
            while self.outbox.active() {
                self.outbox.next().await?;
                self.outbox.advance(&session);
            }
            session.close(cleanup).await
        })
        .await
        .map_err(|_| {
            TuiError::Task("daemon cleanup did not finish before the close deadline".into())
        })?;
        self.outbox = outbox::Outbox::default();
        result
    }

    pub(super) fn send(
        &mut self,
        message: peritus_app_protocol::AppMessage,
    ) -> Result<(), TuiError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| TuiError::Task("request could not be sent while disconnected".into()))?;
        self.outbox.push(session, message)
    }

    pub(super) const fn sending(&self) -> bool {
        self.outbox.active()
    }

    pub(super) fn sent(
        &mut self,
        result: Result<(), TuiError>,
        model: &mut AppModel,
    ) -> Vec<Effect> {
        match result {
            Ok(()) => {
                if let Some(session) = &self.session {
                    self.outbox.advance(session);
                }
                Vec::new()
            }
            Err(error) => {
                self.outbox = outbox::Outbox::default();
                self.session = None;
                model.update(Action::Disconnected(error.to_string()))
            }
        }
    }
}

async fn connect_with_policy(
    endpoint: &std::path::Path,
    protocol: peritus_app_protocol::ProtocolId,
    requested: Option<peritus_types::SessionId>,
    timeout: Option<std::time::Duration>,
    events: mpsc::Sender<ClientEvent>,
) -> Result<ClientSession, TuiError> {
    let operation = ClientSession::connect(endpoint, protocol, requested, events);
    let Some(timeout) = timeout else { return operation.await };
    let deadline = tokio::time::Instant::now().checked_add(timeout).ok_or_else(|| {
        TuiError::InvalidValue("daemon connection timeout is too large for this platform".into())
    })?;
    tokio::time::timeout_at(deadline, operation)
        .await
        .map_err(|_| TuiError::Task("daemon connection reached the caller-selected deadline".into()))?
}

#[cfg(test)]
mod tests;
