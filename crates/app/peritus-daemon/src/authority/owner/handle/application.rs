//! Durable application/session/catalog methods on the bounded authority client.

use peritus_journal::{
    ApplicationArtifact, ApplicationCommandAdmission, ApplicationCommandRecord,
    ApplicationCommandSettlement, ApplicationPrincipal, ApplicationSession,
    ApplicationSessionState, ApplicationWorkspace, ApplicationWorkspaceState,
    NewApplicationArtifact, NewApplicationCommand, NewApplicationPrincipal, NewApplicationSession,
    NewApplicationWorkspace,
};
use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, Sha256Digest, WorkspaceId};
use tokio::sync::oneshot;

use super::{AuthorityHandle, AuthorityMessage};
use crate::{
    DaemonError,
    domain::{DomainOutcome, DomainSubmission},
};

impl AuthorityHandle {
    /// Installs an immutable one-to-one local principal binding.
    pub async fn bind_principal(
        &self,
        principal: NewApplicationPrincipal,
    ) -> Result<ApplicationPrincipal, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::BindPrincipal { principal, respond }, receive).await
    }

    /// Opens one durable negotiated application session.
    pub async fn open_session(
        &self,
        session: NewApplicationSession,
    ) -> Result<ApplicationSession, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::OpenSession { session, respond }, receive).await
    }

    /// Records the exact last successful negotiation of one active actor-owned session.
    ///
    /// # Errors
    ///
    /// Returns a typed authority or storage failure.
    pub async fn observe_session(
        &self,
        session_id: SessionId,
        actor_id: ActorId,
        protocol_id: [u8; 16],
        version_major: u16,
        version_minor: u16,
    ) -> Result<ApplicationSession, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::ObserveSession {
                session_id,
                actor_id,
                protocol_id,
                version_major,
                version_minor,
                respond,
            },
            receive,
        )
        .await
    }

    /// Advances one durable application session lifecycle.
    pub async fn advance_session(
        &self,
        session_id: SessionId,
        state: ApplicationSessionState,
    ) -> Result<ApplicationSession, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::AdvanceSession { session_id, state, respond }, receive).await
    }

    /// Classifies or inserts one durable application command.
    pub async fn admit_command(
        &self,
        command: NewApplicationCommand,
    ) -> Result<ApplicationCommandAdmission, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::AdmitCommand { command, respond }, receive).await
    }

    /// Decodes, reduces, and durably commits one explicitly registered B3 command family.
    pub(crate) async fn dispatch_command(
        &self,
        submission: DomainSubmission,
    ) -> Result<DomainOutcome, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::DispatchCommand { submission, respond }, receive).await
    }

    /// Applies an exact checked durable command settlement.
    pub async fn settle_command(
        &self,
        command_id: CommandId,
        request_digest: Sha256Digest,
        settlement: ApplicationCommandSettlement,
    ) -> Result<ApplicationCommandRecord, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::SettleCommand { command_id, request_digest, settlement, respond },
            receive,
        )
        .await
    }

    /// Reconciles an admitted command against the immutable C0 command record.
    ///
    /// # Errors
    ///
    /// Returns a conflict for digest drift or a typed storage failure.
    pub async fn reconcile_command(
        &self,
        command_id: CommandId,
        request_digest: Sha256Digest,
        domain_command_digest: Sha256Digest,
    ) -> Result<ApplicationCommandRecord, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::ReconcileCommand {
                command_id,
                request_digest,
                domain_command_digest,
                respond,
            },
            receive,
        )
        .await
    }

    /// Loads a bounded deterministic command-recovery batch.
    pub async fn recover_commands(
        &self,
        maximum: usize,
    ) -> Result<Vec<ApplicationCommandRecord>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::RecoverCommands { maximum, respond }, receive).await
    }

    /// Inserts one exact pending artifact catalog row.
    pub async fn begin_artifact(
        &self,
        artifact: NewApplicationArtifact,
    ) -> Result<ApplicationArtifact, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::BeginArtifact { artifact, respond }, receive).await
    }

    /// Publishes artifact availability at one exact journal position.
    pub async fn complete_artifact(
        &self,
        artifact_id: ArtifactId,
        producing_position: u64,
    ) -> Result<ApplicationArtifact, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::CompleteArtifact { artifact_id, producing_position, respond },
            receive,
        )
        .await
    }

    /// Registers exact durable workspace bytes.
    pub async fn register_workspace(
        &self,
        workspace: NewApplicationWorkspace,
    ) -> Result<ApplicationWorkspace, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::RegisterWorkspace { workspace, respond }, receive).await
    }

    /// Changes retained workspace availability state.
    pub async fn set_workspace_state(
        &self,
        workspace_id: WorkspaceId,
        state: ApplicationWorkspaceState,
    ) -> Result<ApplicationWorkspace, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::SetWorkspaceState { workspace_id, state, respond }, receive)
            .await
    }
}
