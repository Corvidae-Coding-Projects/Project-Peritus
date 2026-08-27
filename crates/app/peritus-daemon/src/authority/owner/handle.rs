//! Cloneable bounded client and lifecycle/storage-neutral requests.

mod application;
mod artifact;

use peritus_app_protocol::DaemonStatus;
use peritus_journal::{
    ApplicationPrincipal, ApplicationSession, GlobalEventWindow, OutboxId, OutboxMessage,
};
use peritus_types::{SessionId, Sha256Digest};
use tokio::sync::{mpsc, oneshot};

use super::{super::message::AuthorityMessage, error::owner_stopped};
use crate::{DaemonError, StartupPhase};

/// Cloneable bounded client for the single serialized authority task.
#[derive(Clone, Debug)]
pub struct AuthorityHandle {
    sender: mpsc::Sender<AuthorityMessage>,
}

impl AuthorityHandle {
    pub(super) const fn new(sender: mpsc::Sender<AuthorityMessage>) -> Self {
        Self { sender }
    }

    /// Reads the current truthful bounded daemon status.
    pub async fn status(&self) -> Result<DaemonStatus, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::Status { respond }, receive).await
    }

    /// Reads one durable principal binding by authenticated digest.
    pub async fn principal(
        &self,
        digest: Sha256Digest,
    ) -> Result<Option<ApplicationPrincipal>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::Principal { digest, respond }, receive).await
    }

    /// Reads one durable negotiated session.
    pub async fn session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<ApplicationSession>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::Session { session_id, respond }, receive).await
    }

    /// Reads the current positive durable authority epoch.
    pub async fn authority_epoch(&self) -> Result<Option<u64>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::AuthorityEpoch { respond }, receive).await
    }

    /// Reads one exact bounded global event window.
    pub async fn global_events_after(
        &self,
        cursor: u64,
        maximum: usize,
    ) -> Result<GlobalEventWindow, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::Events { cursor, maximum, respond }, receive).await
    }

    /// Claims the next durable outbox row under an exact positive lease window.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle, clock, or storage failure.
    pub async fn claim_outbox(
        &self,
        now: u64,
        lease_until: u64,
    ) -> Result<Option<OutboxMessage>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::ClaimOutbox { now, lease_until, respond }, receive).await
    }

    /// Acknowledges one successfully settled outbox delivery under its exact claim fence.
    ///
    /// # Errors
    ///
    /// Returns a typed stale-fence or storage failure.
    pub async fn acknowledge_outbox(&self, id: OutboxId, fence: u64) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::AcknowledgeOutbox { id, fence, respond }, receive).await
    }

    /// Advances the monotonic daemon startup checkpoint.
    pub async fn advance_startup(&self, phase: StartupPhase) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::AdvanceStartup { phase, respond }, receive).await
    }

    /// Enters explicit read-only diagnostic service.
    pub async fn enter_read_only(&self, diagnostic: String) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::ReadOnly { diagnostic, respond }, receive).await
    }

    /// Closes new mutation intake while accepted work drains.
    pub async fn begin_draining(&self) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::BeginDraining { respond }, receive).await
    }

    /// Closes the owner after all earlier queued messages are handled.
    pub async fn stop(&self) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::Stop { respond }, receive).await
    }

    pub(super) async fn send<T>(
        &self,
        message: AuthorityMessage,
        receive: oneshot::Receiver<Result<T, DaemonError>>,
    ) -> Result<T, DaemonError> {
        self.sender.send(message).await.map_err(|_| owner_stopped())?;
        receive.await.map_err(|_| owner_stopped())?
    }
}
