//! Closed messages accepted by the single writable-state owner.

use peritus_app_protocol::{
    ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata, ArtifactOpenRequest,
    DaemonStatus, PromptAnswer, PromptBinding, PromptCancellation, PromptCorrelation, RequestId,
    TransferId,
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
use crate::outbox::OrchestratorDirectiveClaim;
use crate::{DaemonError, PromptTerminalStatus, StartupPhase};

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
    SettleOrchestratorDirective {
        claim: OrchestratorDirectiveClaim,
        respond: Response<()>,
    },
    DeliverOrchestratorChild {
        claim: OrchestratorDirectiveClaim,
        respond: Response<()>,
    },
    QueryEvaluation {
        campaign: peritus_eval::EvaluationCampaignId,
        respond: Response<Option<peritus_eval::EvaluationState>>,
    },
    StartEvaluation {
        campaign: peritus_eval::EvaluationCampaignId,
        profile: Box<peritus_eval::FrozenEvaluationProfile>,
        respond: Response<peritus_eval::EvaluationState>,
    },
    CancelEvaluation {
        campaign: peritus_eval::EvaluationCampaignId,
        reason: Sha256Digest,
        respond: Response<peritus_eval::EvaluationState>,
    },
    DeliverEvaluationSchedule {
        claim: peritus_eval::ScheduleDirectiveClaim,
        respond: Response<()>,
    },
    BeginEvaluation {
        claim: peritus_eval::ExecutionDirectiveClaim,
        cancellation: std::sync::Arc<std::sync::atomic::AtomicBool>,
        respond: Response<Option<crate::rsi::authority::EvaluationWork>>,
    },
    FinishEvaluation {
        claim: peritus_eval::ExecutionDirectiveClaim,
        record: peritus_eval::RolloutRecord,
        processes: peritus_process::ProcessStore,
        state_root: std::path::PathBuf,
        respond: Response<()>,
    },
    RetainEvaluationArtifacts {
        claim: peritus_eval::ExecutionDirectiveClaim,
        roots: Vec<peritus_artifact_store::ArtifactDigest>,
        respond: Response<()>,
    },
    PublishEvaluation {
        claim: peritus_eval::PublicationDirectiveClaim,
        database: std::path::PathBuf,
        respond: Response<()>,
    },
    FailEvaluation {
        claim: peritus_eval::EvaluationDirectiveClaim,
        detail: String,
        processes: peritus_process::ProcessStore,
        config: Box<crate::DaemonConfig>,
        respond: Response<()>,
    },
    PublishEvolution {
        claim: peritus_evolution::EvolutionPublicationClaim,
        database: std::path::PathBuf,
        maximum_bytes: u64,
        respond: Response<()>,
    },
    PublishDebugger {
        claim: peritus_debugger::PublicationDirectiveClaim,
        database: std::path::PathBuf,
        state_root: std::path::PathBuf,
        maximum_bytes: u64,
        respond: Response<()>,
    },
    RegisterPrompt {
        actor_id: ActorId,
        session_id: SessionId,
        binding: PromptBinding,
        maximum_answer_bytes: usize,
        respond: Response<PromptTerminalStatus>,
    },
    AnswerPrompt {
        actor_id: ActorId,
        session_id: SessionId,
        request_id: RequestId,
        answer: PromptAnswer,
        request_frame: Vec<u8>,
        respond: Response<PromptTerminalStatus>,
    },
    CancelPrompt {
        actor_id: ActorId,
        session_id: SessionId,
        request_id: RequestId,
        cancellation: PromptCancellation,
        request_frame: Vec<u8>,
        respond: Response<PromptTerminalStatus>,
    },
    PromptStatus {
        actor_id: ActorId,
        session_id: SessionId,
        correlation: PromptCorrelation,
        respond: Response<PromptTerminalStatus>,
    },
    RetirePrompt {
        actor_id: ActorId,
        session_id: SessionId,
        correlation: PromptCorrelation,
        respond: Response<PromptTerminalStatus>,
    },
    PromptCorrelations {
        actor_id: ActorId,
        session_id: SessionId,
        maximum: usize,
        respond: Response<Vec<PromptCorrelation>>,
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
    BeginScopedArtifactUpload {
        actor_id: ActorId,
        session_id: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
        scope: crate::artifact::ArtifactScope,
        respond: Response<()>,
    },
    ReadScopedArtifact {
        scope: crate::artifact::ArtifactScope,
        artifact_id: ArtifactId,
        maximum_bytes: u64,
        respond: Response<(ApplicationArtifact, Vec<u8>)>,
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
    ReconcileWorkspaces {
        config: Box<crate::DaemonConfig>,
        admitted: Vec<crate::config::FolderDeclaration>,
        respond: Response<crate::startup::workspace::WorkspaceCatalog>,
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
