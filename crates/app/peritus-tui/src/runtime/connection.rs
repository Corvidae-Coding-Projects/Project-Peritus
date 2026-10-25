//! An owned connection attempt is polled beside input, never in front of the UI loop.

use super::{CONNECT_TIMEOUT, TuiConfig, process_seed, protocol_id};
use crate::{
    TuiError,
    action::{Action, Effect},
    client::{ClientEvent, ClientSession},
    model::AppModel,
};
use std::{future::Future, pin::Pin};
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc;
use tokio::time::Instant;

mod outbox;

type Attempt = Pin<Box<dyn Future<Output = Result<ClientSession, TuiError>> + Send>>;

pub(super) struct Connection {
    pub(super) session: Option<ClientSession>,
    attempt: Option<Attempt>,
    generation: u64,
    events: mpsc::Sender<ClientEvent>,
    outbox: outbox::Outbox,
    retry_at: Option<Instant>,
    retry_delay: Duration,
    connected_at: Option<Instant>,
    diagnostic_log: Option<PathBuf>,
}

impl Connection {
    pub(super) fn new(events: mpsc::Sender<ClientEvent>, diagnostic_log: Option<PathBuf>) -> Self {
        Self {
            session: None,
            attempt: None,
            generation: 0,
            events,
            outbox: outbox::Outbox::default(),
            retry_at: None,
            retry_delay: Duration::from_millis(500),
            connected_at: None,
            diagnostic_log,
        }
    }

    pub(super) fn start(&mut self, config: &TuiConfig, model: &mut AppModel) {
        self.retry_at = None;
        self.outbox = outbox::Outbox::default();
        self.generation = self.generation.saturating_add(1);
        let protocol = protocol_id(process_seed(config.endpoint()), self.generation);
        let endpoint = config.endpoint().to_owned();
        let requested = model.retained_session().or_else(|| config.requested_session());
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
            tokio::time::timeout(
                CONNECT_TIMEOUT,
                ClientSession::connect(&endpoint, protocol?, requested, events),
            )
            .await
            .map_err(|_| {
                TuiError::Task(format!(
                    "connection timed out after {} seconds",
                    CONNECT_TIMEOUT.as_secs()
                ))
            })?
        }));
    }

    pub(super) const fn active(&self) -> bool {
        self.attempt.is_some() || self.retry_at.is_some()
    }

    pub(super) async fn next(&mut self, config: &TuiConfig, model: &mut AppModel) -> Vec<Effect> {
        if let Some(attempt) = self.attempt.as_mut() {
            let result = attempt.await;
            self.attempt = None;
            self.finish(result, model)
        } else if let Some(deadline) = self.retry_at {
            tokio::time::sleep_until(deadline).await;
            self.start(config, model);
            Vec::new()
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
                self.connected_at = Some(Instant::now());
                super::diagnostics::record(
                    self.diagnostic_log.as_deref(),
                    "connected",
                    self.generation,
                    "daemon session established",
                );
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
            Err(error) => self.disconnected("connect", &error.to_string(), model),
        }
    }

    pub(super) fn cancel(&mut self) {
        self.attempt = None;
        self.retry_at = None;
        self.outbox = outbox::Outbox::default();
    }

    pub(super) async fn close(
        &mut self,
        cleanup: Vec<peritus_app_protocol::AppMessage>,
    ) -> Result<(), TuiError> {
        self.attempt = None;
        self.retry_at = None;
        let Some(session) = self.session.take() else {
            self.cancel();
            return Ok(());
        };
        let result = tokio::time::timeout(CONNECT_TIMEOUT, async {
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
            Err(error) => self.disconnected("enqueue", &error.to_string(), model),
        }
    }

    pub(super) fn disconnected(
        &mut self,
        stage: &'static str,
        error: &str,
        model: &mut AppModel,
    ) -> Vec<Effect> {
        // Both stream owners can report one broken connection. Schedule recovery
        // once, preserving the first diagnostic and its pending-intent reconciliation.
        if self.retry_at.is_some() {
            return Vec::new();
        }
        if self
            .connected_at
            .take()
            .is_some_and(|connected| connected.elapsed() >= Duration::from_secs(10))
        {
            self.retry_delay = Duration::from_millis(500);
        }
        self.cancel();
        self.session = None;
        let delay = self.retry_delay;
        self.retry_at = Some(Instant::now() + delay);
        self.retry_delay = (delay * 2).min(Duration::from_secs(8));
        super::diagnostics::record(self.diagnostic_log.as_deref(), stage, self.generation, error);
        // This invalidates old transport requests but retains the selected run,
        // unsent draft and revisioned operations whose receipts need reconciliation.
        // No Retry, message or control action is resubmitted by this connection owner.
        let detail = format!("{error}; reconnecting automatically in {} ms", delay.as_millis());
        model.update(if stage == "connect" {
            Action::ConnectionFailed(detail)
        } else {
            Action::Disconnected(detail)
        })
    }
}
