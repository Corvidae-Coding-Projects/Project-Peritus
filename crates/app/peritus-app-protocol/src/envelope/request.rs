//! Closed typed application request envelope.

use crate::{
    AppErrorCode, AppProtocolError, ArtifactCancellation, ArtifactChunk, ArtifactCompletion,
    ArtifactMetadata, CommandBinding, CorrelationId, EventCursor, ProductRunContinuation,
    ProductRunControl, ProductRunConversationQuery, ProductRunQuery, ProductRunRequest,
    PromptAnswer, PromptCancellation, RequestId, ShutdownRequest, SubscriptionFilter,
    SubscriptionId, TerminalBinding, TerminalCancellation, TerminalDetach, TerminalInput,
    TerminalResize, TransferId,
};
use peritus_types::ArtifactId;

use super::ProtocolContext;

/// Checked event-subscription creation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionRequest {
    subscription_id: SubscriptionId,
    filter: SubscriptionFilter,
    after: EventCursor,
    maximum_in_flight: u32,
    snapshot_acceptable: bool,
}

impl SubscriptionRequest {
    /// Creates a subscription request with a positive negotiated delivery window.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidLimits`] for a zero window.
    pub fn new(
        subscription_id: SubscriptionId,
        filter: SubscriptionFilter,
        after: EventCursor,
        maximum_in_flight: u32,
        snapshot_acceptable: bool,
    ) -> Result<Self, AppProtocolError> {
        if maximum_in_flight == 0 {
            Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None))
        } else {
            Ok(Self { subscription_id, filter, after, maximum_in_flight, snapshot_acceptable })
        }
    }

    /// Returns the new subscription identity.
    #[must_use]
    pub const fn subscription_id(&self) -> SubscriptionId {
        self.subscription_id
    }
    /// Borrows the canonical topic filter.
    #[must_use]
    pub const fn filter(&self) -> &SubscriptionFilter {
        &self.filter
    }
    /// Returns the cursor after which delivery begins.
    #[must_use]
    pub const fn after(&self) -> EventCursor {
        self.after
    }
    /// Returns the requested positive in-flight window.
    #[must_use]
    pub const fn maximum_in_flight(&self) -> u32 {
        self.maximum_in_flight
    }
    /// Returns whether a retained snapshot is an acceptable gap recovery.
    #[must_use]
    pub const fn snapshot_acceptable(&self) -> bool {
        self.snapshot_acceptable
    }
}

/// Request to open one exact artifact transfer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactOpenRequest {
    transfer_id: TransferId,
    artifact_id: ArtifactId,
}

impl ArtifactOpenRequest {
    /// Creates an exact artifact-open request.
    #[must_use]
    pub const fn new(transfer_id: TransferId, artifact_id: ArtifactId) -> Self {
        Self { transfer_id, artifact_id }
    }
    /// Returns the caller-selected transfer identity.
    #[must_use]
    pub const fn transfer_id(self) -> TransferId {
        self.transfer_id
    }
    /// Returns the requested artifact identity.
    #[must_use]
    pub const fn artifact_id(self) -> ArtifactId {
        self.artifact_id
    }
}

/// Closed schema-v1 application request payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppRequestPayload {
    /// Previews exact covered targets, exclusions, conflicts, and non-restorable effects.
    PreviewWorkbenchRewind(crate::WorkbenchRewindRequest),
    /// Inspects one exact durable checkpoint manifest without mutating current state.
    InspectWorkbenchCheckpoint(crate::WorkbenchRewindRequest),
    /// Inspects bounded active guidance and optional content-free tombstones.
    QueryWorkbenchMemory(crate::WorkbenchMemoryQuery),
    /// Discovers bounded local project controls and an exact inert instruction-file proposal.
    DiscoverInit(crate::InitDiscoveryRequest),
    /// Inspects the effective host-intersected workspace policy without mutation.
    QueryWorkbenchPermissions(crate::WorkbenchQuery),
    /// Builds a deterministic local C6-validated prompt-view proposal without mutation.
    PreviewWorkbenchCompaction(crate::WorkbenchCompactionRequest),
    /// Searches the durable local conversation library without inference.
    QueryConversationLibrary(crate::ConversationLibraryQuery),
    /// Inspects one durable bounded goal and its cumulative accounting without starting work.
    QueryWorkbenchGoal(crate::WorkbenchQuery),
    /// Inspects source/build/process/capture identities and independent validation evidence.
    QueryWorkbenchResult(crate::WorkbenchResultQuery),
    /// Inspects the exact structured candidate diff, anchored comments, and evidence freshness.
    QueryWorkbenchReview(crate::WorkbenchReviewQuery),
    /// Begins explicit selected-text transfer scoped to the conversation.
    BeginWorkbenchFileUpload(crate::WorkbenchFileUpload),
    /// Validates an uploaded immutable text snapshot, without reopening its source label.
    PreviewWorkbenchFileImport(crate::WorkbenchFileImportRequest),
    /// Inspects retained image metadata and current selection without importing or inference.
    QueryWorkbenchImages(crate::WorkbenchImageQuery),
    /// Begins bounded image transfer after exact owner/conversation/workspace authorization.
    BeginWorkbenchImageUpload(crate::WorkbenchImageUpload),
    /// Locally validates completed uploaded bytes against an explicit configured provider.
    PreviewWorkbenchImage(crate::WorkbenchImageRequest),
    /// Previews one authorized selected-workspace file/range without inference.
    PreviewWorkbenchFile(crate::WorkbenchFileRequest),
    /// Inspects retained file versions and explicit future selection.
    QueryWorkbenchFiles(crate::WorkbenchFileQuery),
    /// Inspects the exact user-confirmed brief without starting work.
    QueryWorkbenchBrief(crate::WorkbenchQuery),
    /// Inspects eligible inputs or a sealed invocation manifest without inference or recovery.
    QueryWorkbenchContext(crate::WorkbenchContextQuery),
    /// Inspects one revision-fenced page of pending inputs or immutable input history.
    QueryWorkbenchQueue(crate::WorkbenchQueueQuery),
    /// Admits one exact revisioned user-control intent through the authoritative journal.
    WorkbenchCommand(crate::WorkbenchCommand),
    /// Inspects selected durable conversation metadata without inference.
    QueryWorkbench(crate::WorkbenchQuery),
    /// Resolves the original receipt for an exact actor-bound operation without reapplying it.
    QueryWorkbenchReceipt(crate::WorkbenchCommand),
    /// Inspects bounded local prerequisites without inference, repair, or network probes.
    Doctor(crate::DoctorQuery),
    /// Durably selects models for subsequent turns of an existing conversation.
    UpdateModels(crate::ProductModelUpdate),
    /// Starts or steers a conversation with explicit execution semantics.
    Interact(crate::ProductInteractionRequest),
    /// Reads public activity and exact input incorporation status.
    QueryInteraction(ProductRunConversationQuery),
    /// Discovers models from one configured provider route.
    QueryModels(crate::ProductModelQuery),
    /// Submits one exact, idempotent B3 command binding.
    SubmitCommand(CommandBinding),
    /// Starts or resumes one event subscription.
    Subscribe(SubscriptionRequest),
    /// Opens a bounded artifact transfer.
    OpenArtifact(ArtifactOpenRequest),
    /// Cancels an artifact transfer.
    CancelArtifact(ArtifactCancellation),
    /// Begins one bounded artifact upload from exact declared metadata.
    BeginArtifactUpload(ArtifactMetadata),
    /// Supplies one contiguous artifact-upload chunk.
    UploadArtifactChunk(ArtifactChunk),
    /// Completes one artifact upload with exact size and digest.
    CompleteArtifactUpload(ArtifactCompletion),
    /// Starts one daemon-owned writer-reviewer-fixer coding run.
    StartProductRun(ProductRunRequest),
    /// Cancels or retries one exact product run.
    ControlProductRun(ProductRunControl),
    /// Queries recent or exact product-run observations.
    QueryProductRuns(ProductRunQuery),
    /// Adds user context to an active or resumable product run.
    ContinueProductRun(ProductRunContinuation),
    /// Queries the conversation for one exact product run.
    QueryProductRunConversation(ProductRunConversationQuery),
    /// Answers an approval or user-input prompt.
    AnswerPrompt(PromptAnswer),
    /// Cancels an outstanding prompt.
    CancelPrompt(PromptCancellation),
    /// Attaches to one C2-owned terminal process.
    AttachTerminal(TerminalBinding),
    /// Sends bounded terminal input.
    TerminalInput(TerminalInput),
    /// Resizes an attached terminal.
    TerminalResize(TerminalResize),
    /// Detaches an attached terminal.
    DetachTerminal(TerminalDetach),
    /// Cancels an attached terminal.
    CancelTerminal(TerminalCancellation),
    /// Requests current read-only daemon status.
    DaemonStatus,
    /// Requests graceful daemon shutdown without implying acceptance.
    Shutdown(ShutdownRequest),
}

impl AppRequestPayload {
    /// Returns the independently negotiated capability required by additive workbench operations.
    /// Legacy payloads retain their original admission contracts.
    #[must_use]
    pub const fn required_workbench_feature(&self) -> Option<crate::WellKnownProtocolFeature> {
        match self {
            Self::PreviewWorkbenchRewind(_) | Self::InspectWorkbenchCheckpoint(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchCheckpoints)
            }
            Self::QueryWorkbenchMemory(_) => Some(crate::WellKnownProtocolFeature::WorkbenchMemory),
            Self::DiscoverInit(_) => Some(crate::WellKnownProtocolFeature::WorkbenchInit),
            Self::QueryWorkbenchPermissions(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchPermissions)
            }
            Self::PreviewWorkbenchCompaction(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchCompaction)
            }
            Self::QueryWorkbenchResult(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchPreview)
            }
            Self::QueryWorkbenchReview(_) => Some(crate::WellKnownProtocolFeature::WorkbenchReview),
            Self::QueryConversationLibrary(_) => {
                Some(crate::WellKnownProtocolFeature::ConversationLibrary)
            }
            Self::PreviewWorkbenchFile(_)
            | Self::QueryWorkbenchFiles(_)
            | Self::BeginWorkbenchFileUpload(_)
            | Self::PreviewWorkbenchFileImport(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchFiles)
            }
            Self::QueryWorkbenchImages(_)
            | Self::BeginWorkbenchImageUpload(_)
            | Self::PreviewWorkbenchImage(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchImages)
            }
            Self::QueryWorkbenchBrief(_) => Some(crate::WellKnownProtocolFeature::WorkbenchBrief),
            Self::QueryWorkbenchGoal(_) => Some(crate::WellKnownProtocolFeature::WorkbenchGoals),
            Self::QueryWorkbenchContext(_) => {
                Some(crate::WellKnownProtocolFeature::WorkbenchContext)
            }
            Self::QueryWorkbenchQueue(_) => Some(crate::WellKnownProtocolFeature::WorkbenchInputs),
            Self::Doctor(_) => Some(crate::WellKnownProtocolFeature::ProductDiagnostics),
            Self::WorkbenchCommand(command) | Self::QueryWorkbenchReceipt(command) => {
                Some(required_workbench_intent_feature(command.intent()))
            }
            Self::QueryWorkbench(_) => Some(crate::WellKnownProtocolFeature::WorkbenchControl),
            _ => None,
        }
    }
}

const fn required_workbench_intent_feature(
    intent: &crate::WorkbenchIntent,
) -> crate::WellKnownProtocolFeature {
    use crate::{WellKnownProtocolFeature as Feature, WorkbenchIntent as Intent};

    match intent {
        Intent::SetPermissions(_) => Feature::WorkbenchPermissions,
        Intent::SaveGuidance(_)
        | Intent::ReviseGuidance(_)
        | Intent::PinGuidance(_)
        | Intent::ScopeGuidance(_)
        | Intent::ForgetGuidance(_) => Feature::WorkbenchMemory,
        Intent::ApplyInitDiff(_) => Feature::WorkbenchInit,
        Intent::ForkConversation(_) => Feature::ConversationForks,
        Intent::AttachFile { .. } | Intent::AttachFileImport { .. } | Intent::SelectFile { .. } => {
            Feature::WorkbenchFiles
        }
        Intent::AttachImage { .. } | Intent::SelectImage { .. } => Feature::WorkbenchImages,
        Intent::SetBrief { .. } | Intent::AcceptBriefProposal { .. } => Feature::WorkbenchBrief,
        Intent::SetContext { .. } => Feature::WorkbenchContext,
        Intent::ApplyCompaction(_) => Feature::WorkbenchCompaction,
        Intent::Queue(_) => Feature::WorkbenchInputs,
        Intent::StartExecution(_) => Feature::WorkbenchExecution,
        Intent::StartGoal { .. }
        | Intent::PauseGoal { .. }
        | Intent::ResumeGoal { .. }
        | Intent::ClearGoal { .. } => Feature::WorkbenchGoals,
        Intent::UpdateGoalBudget { .. } => Feature::WorkbenchBudgets,
        Intent::AddReview { .. } | Intent::RebindReview { .. } | Intent::DismissReview { .. } => {
            Feature::WorkbenchReview
        }
        Intent::StartPreview(_)
        | Intent::InteractPreview { .. }
        | Intent::CapturePreview(_)
        | Intent::StopPreview { .. }
        | Intent::CheckPreviewBehavior { .. }
        | Intent::AddArtifactFeedback { .. } => Feature::WorkbenchPreview,
        Intent::CreateCheckpoint(_) | Intent::ApplyRewind(_) => Feature::WorkbenchCheckpoints,
        Intent::CreateConversation(_)
        | Intent::RenameConversation(_)
        | Intent::PinConversation(_)
        | Intent::ArchiveConversation(_) => Feature::WorkbenchControl,
    }
}

/// Complete typed request envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppRequestEnvelope {
    context: ProtocolContext,
    request_id: RequestId,
    correlation_id: CorrelationId,
    payload: AppRequestPayload,
}

impl AppRequestEnvelope {
    /// Creates a request and checks duplicated command/shutdown correlation fields.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::CommandBindingMismatch`] when an inner binding disagrees with the
    /// outer request or correlation identity.
    pub fn new(
        context: ProtocolContext,
        request_id: RequestId,
        correlation_id: CorrelationId,
        payload: AppRequestPayload,
    ) -> Result<Self, AppProtocolError> {
        let matches = match &payload {
            AppRequestPayload::SubmitCommand(command) => {
                command.request_id() == request_id
                    && command.correlation_id() == correlation_id
                    && command.session_id() == context.session_id()
            }
            AppRequestPayload::Shutdown(shutdown) => {
                shutdown.request_id() == request_id && shutdown.correlation_id() == correlation_id
            }
            _ => true,
        };
        if !matches {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        Ok(Self { context, request_id, correlation_id, payload })
    }

    /// Returns the negotiated context.
    #[must_use]
    pub const fn context(&self) -> ProtocolContext {
        self.context
    }
    /// Returns the request identity.
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }
    /// Returns the correlation identity.
    #[must_use]
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
    /// Borrows the closed request payload.
    #[must_use]
    pub const fn payload(&self) -> &AppRequestPayload {
        &self.payload
    }
}
