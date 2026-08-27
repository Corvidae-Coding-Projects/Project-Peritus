//! Closed messages accepted by the single writable-state owner.

use peritus_app_protocol::{
    ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest,
    DaemonStatus, TransferId,
};
use peritus_journal::{
    ApplicationArtifact, ApplicationCommandAdmission, ApplicationCommandRecord,
    ApplicationCommandSettlement, ApplicationPrincipal, ApplicationSession,
    ApplicationSessionState, ApplicationWorkspace, ApplicationWorkspaceState, GlobalEventWindow,
    NewApplicationArtifact, NewApplicationCommand, NewApplicationPrincipal, NewApplicationSession,
    NewApplicationWorkspace, OutboxId, OutboxMessage,
};
use peritus_types::{ActorId, ArtifactId, CommandId, SessionId, Sha256Digest, WorkspaceId};
use tokio::sync::oneshot;

use crate::domain::{DomainOutcome, DomainSubmission};
use crate::{DaemonError, StartupPhase};

pub(super) enum AuthorityMessage {
    Status {
        respond: Response<DaemonStatus>,
    },
    Principal {
        digest: Sha256Digest,
        respond: Response<Option<ApplicationPrincipal>>,
    },
    Session {
        session_id: SessionId,
        respond: Response<Option<ApplicationSession>>,
    },
    AuthorityEpoch {
        respond: Response<Option<u64>>,
    },
    Events {
        cursor: u64,
        maximum: usize,
        respond: Response<GlobalEventWindow>,
    },
    ClaimOutbox {
        now: u64,
        lease_until: u64,
        respond: Response<Option<OutboxMessage>>,
    },
    AcknowledgeOutbox {
        id: OutboxId,
        fence: u64,
        respond: Response<()>,
    },
    OpenArtifact {
        actor_id: ActorId,
        session_id: SessionId,
        request: ArtifactOpenRequest,
        maximum_chunk_bytes: usize,
        respond: Response<ArtifactMetadata>,
    },
    PollArtifact {
        actor_id: ActorId,
        session_id: SessionId,
        transfer_id: TransferId,
        maximum_chunk_bytes: usize,
        respond: Response<crate::artifact::ArtifactPoll>,
    },
    BeginArtifactUpload {
        actor_id: ActorId,
        session_id: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
        respond: Response<()>,
    },
    UploadArtifactChunk {
        actor_id: ActorId,
        session_id: SessionId,
        chunk: ArtifactChunk,
        respond: Response<()>,
    },
    CompleteArtifactUpload {
        actor_id: ActorId,
        session_id: SessionId,
        completion: ArtifactCompletion,
        respond: Response<()>,
    },
    CancelArtifactTransfer {
        actor_id: ActorId,
        session_id: SessionId,
        cancellation: ArtifactCancellation,
        respond: Response<()>,
    },
    AbandonArtifactTransfers {
        actor_id: ActorId,
        session_id: SessionId,
        transfer_ids: Vec<TransferId>,
        respond: Response<()>,
    },
    BindPrincipal {
        principal: NewApplicationPrincipal,
        respond: Response<ApplicationPrincipal>,
    },
    OpenSession {
        session: NewApplicationSession,
        respond: Response<ApplicationSession>,
    },
    ObserveSession {
        session_id: SessionId,
        actor_id: ActorId,
        protocol_id: [u8; 16],
        version_major: u16,
        version_minor: u16,
        respond: Response<ApplicationSession>,
    },
    AdvanceSession {
        session_id: SessionId,
        state: ApplicationSessionState,
        respond: Response<ApplicationSession>,
    },
    AdmitCommand {
        command: NewApplicationCommand,
        respond: Response<ApplicationCommandAdmission>,
    },
    DispatchCommand {
        submission: DomainSubmission,
        respond: Response<DomainOutcome>,
    },
    SettleCommand {
        command_id: CommandId,
        request_digest: Sha256Digest,
        settlement: ApplicationCommandSettlement,
        respond: Response<ApplicationCommandRecord>,
    },
    ReconcileCommand {
        command_id: CommandId,
        request_digest: Sha256Digest,
        domain_command_digest: Sha256Digest,
        respond: Response<ApplicationCommandRecord>,
    },
    RecoverCommands {
        maximum: usize,
        respond: Response<Vec<ApplicationCommandRecord>>,
    },
    BeginArtifact {
        artifact: NewApplicationArtifact,
        respond: Response<ApplicationArtifact>,
    },
    CompleteArtifact {
        artifact_id: ArtifactId,
        producing_position: u64,
        respond: Response<ApplicationArtifact>,
    },
    RegisterWorkspace {
        workspace: NewApplicationWorkspace,
        respond: Response<ApplicationWorkspace>,
    },
    SetWorkspaceState {
        workspace_id: WorkspaceId,
        state: ApplicationWorkspaceState,
        respond: Response<ApplicationWorkspace>,
    },
    AdvanceStartup {
        phase: StartupPhase,
        respond: Response<()>,
    },
    ReadOnly {
        diagnostic: String,
        respond: Response<()>,
    },
    BeginDraining {
        respond: Response<()>,
    },
    Stop {
        respond: Response<()>,
    },
}

pub(super) type Response<T> = oneshot::Sender<Result<T, DaemonError>>;
