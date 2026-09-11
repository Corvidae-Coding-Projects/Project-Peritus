//! Closed typed workbench mutations; no variant itself authenticates or starts work.

use super::{
    ControlOperationId, ConversationTitle, InitProposal, Sha256Digest, WorkbenchArtifactRegion,
    WorkbenchBriefField, WorkbenchCaptureRequest, WorkbenchCheckpointName,
    WorkbenchCompactionPreview, WorkbenchContextPreference, WorkbenchContextSource,
    WorkbenchExecutionSettings, WorkbenchFileImportPreview, WorkbenchFilePreview,
    WorkbenchForkRequest, WorkbenchGoalBudget, WorkbenchGoalDefinition, WorkbenchGoalPauseMode,
    WorkbenchGuidanceForget, WorkbenchGuidancePin, WorkbenchGuidanceRevision,
    WorkbenchGuidanceSave, WorkbenchGuidanceScopeChange, WorkbenchImagePreview, WorkbenchInputText,
    WorkbenchLaunchProfile, WorkbenchLaunchText, WorkbenchPermissionChange, WorkbenchPreviewInput,
    WorkbenchQueueIntent, WorkbenchReviewAnchor, WorkbenchReviewFeedback, WorkbenchRewindPreview,
};

/// Closed typed metadata changes; none start or resume inference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkbenchIntent {
    /// Creates a new non-running conversation root.
    CreateConversation(ConversationTitle),
    /// Changes only the selected title.
    RenameConversation(ConversationTitle),
    /// Changes library pinning, not authority or mandatory retention.
    PinConversation(bool),
    /// Changes reversible library archive state, not execution eligibility.
    ArchiveConversation(bool),
    /// Creates a non-running child with exact source/checkpoint lineage.
    ForkConversation(WorkbenchForkRequest),
    /// Changes exact pending inputs without starting or resuming execution.
    Queue(WorkbenchQueueIntent),
    /// Explicitly admits execution over the selected conversation's eligible durable inputs.
    StartExecution(WorkbenchExecutionSettings),
    /// Revises one explicitly user-confirmed requirement without starting execution.
    SetBrief {
        /// Exact field selected by the user.
        field: WorkbenchBriefField,
        /// Exact confirmed text, kept as a revisioned input source.
        text: WorkbenchInputText,
    },
    /// Explicitly promotes one exact immutable agent reply into a user-confirmed brief field.
    AcceptBriefProposal {
        /// Field selected by the user; the proposal does not choose its own authority.
        field: WorkbenchBriefField,
        /// Immutable public-reply publication operation.
        proposal: ControlOperationId,
        /// Exact digest the user inspected.
        digest: Sha256Digest,
    },
    /// Pins, excludes, or restores one exact source from a revision-fenced context inspection.
    SetContext {
        /// Exact content-free source identity inspected by the user.
        source: WorkbenchContextSource,
        /// Explicit preference, or none to restore ordinary host policy.
        preference: Option<WorkbenchContextPreference>,
    },
    /// Applies only the exact deterministic local prompt-view proposal the user inspected.
    ApplyCompaction(WorkbenchCompactionPreview),
    /// Confirms the exact image preview and caption, without starting a provider request.
    AttachImage {
        /// Exact host preview, revalidated before acceptance.
        preview: WorkbenchImagePreview,
        /// User-approved caption, recorded as an immutable queue input.
        text: WorkbenchInputText,
    },
    /// Revises only future image selection; sealed request history is unchanged.
    SelectImage {
        /// Original image import operation in this conversation.
        attachment: ControlOperationId,
        /// Desired future inclusion preference.
        selected: bool,
    },
    /// Confirms exact authorized source bytes without starting inference.
    AttachFile {
        /// Exact provider-bound source preview, revalidated by the host.
        preview: WorkbenchFilePreview,
        /// User-confirmed caption and queue instruction.
        text: WorkbenchInputText,
    },
    /// Confirms exact client-imported text bytes; the source label never becomes path authority.
    AttachFileImport {
        /// Exact validated uploaded-text preview.
        preview: WorkbenchFileImportPreview,
        /// User-confirmed caption and queue instruction.
        text: WorkbenchInputText,
    },
    /// Revises only future file inclusion.
    SelectFile {
        /// Original reference operation in this conversation.
        attachment: ControlOperationId,
        /// Desired future inclusion preference.
        selected: bool,
    },
    /// Confirms the displayed goal definition and starts the existing product runner.
    StartGoal {
        /// Exact objective, typed criteria, and cumulative user limits.
        definition: WorkbenchGoalDefinition,
        /// Existing runner provider/mode/model binding.
        settings: WorkbenchExecutionSettings,
    },
    /// Requests a durable execution pause at a named safe boundary.
    PauseGoal {
        /// Exact goal identity (the original start operation).
        goal: ControlOperationId,
        /// Selected boundary semantics.
        mode: WorkbenchGoalPauseMode,
    },
    /// Explicitly resumes the same logical goal with accounting retained.
    ResumeGoal {
        /// Exact goal identity (the original start operation).
        goal: ControlOperationId,
    },
    /// Replaces cumulative user limits without implicitly resuming.
    UpdateGoalBudget {
        /// Exact goal identity (the original start operation).
        goal: ControlOperationId,
        /// Complete replacement user-limit set.
        budget: WorkbenchGoalBudget,
    },
    /// Cancels future goal continuation while retaining history and effects.
    ClearGoal {
        /// Exact goal identity (the original start operation).
        goal: ControlOperationId,
    },
    /// Adds one typed digest-bound file or hunk comment.
    AddReview {
        /// Exact target selected from a current structured diff response.
        anchor: WorkbenchReviewAnchor,
        /// Closed user meaning.
        feedback: WorkbenchReviewFeedback,
        /// Exact user-authored comment.
        message: WorkbenchInputText,
    },
    /// Explicitly rebinds a stale comment to a newly inspected exact target.
    RebindReview {
        /// Stable original add-operation identity.
        comment: ControlOperationId,
        /// Newly selected current target.
        anchor: WorkbenchReviewAnchor,
    },
    /// Explicitly dismisses one comment or hard constraint.
    DismissReview {
        /// Stable original add-operation identity.
        comment: ControlOperationId,
    },
    /// Explicitly authorizes one bounded daemon-owned preview launch.
    StartPreview(WorkbenchLaunchProfile),
    /// Sends exact bounded input to one live owned preview.
    InteractPreview {
        /// Original launch operation.
        launch: ControlOperationId,
        /// Exact bytes to send to the authorized process input.
        input: WorkbenchPreviewInput,
    },
    /// Requests a capability-checked capture of one explicit selected app window.
    CapturePreview(WorkbenchCaptureRequest),
    /// Explicitly terminates one owned preview process tree.
    StopPreview {
        /// Original launch operation.
        launch: ControlOperationId,
    },
    /// Records a behavior check only when retained process output contains the exact observation.
    CheckPreviewBehavior {
        /// Original launch operation.
        launch: ControlOperationId,
        /// Exact output observation the daemon must verify.
        observed: WorkbenchLaunchText,
        /// Exact graphical criterion description; unrelated notes never qualify a goal.
        note: WorkbenchInputText,
    },
    /// Adds human feedback anchored to one immutable captured artifact.
    AddArtifactFeedback {
        /// Exact capture operation whose digest remains authoritative.
        capture: ControlOperationId,
        /// Reused closed P3 feedback meaning.
        feedback: WorkbenchReviewFeedback,
        /// Exact user-authored feedback.
        message: WorkbenchInputText,
        /// Optional pixel-space region within the exact capture.
        region: Option<WorkbenchArtifactRegion>,
    },
    /// Captures selected whole workspace files and historical references at a safe boundary.
    CreateCheckpoint(WorkbenchCheckpointName),
    /// Applies only an exact inspected restore preview after explicit confirmation.
    ApplyRewind(WorkbenchRewindPreview),
    /// Narrows or restores one capability within the immutable host ceiling.
    SetPermissions(WorkbenchPermissionChange),
    /// Saves exact explicitly user-approved project-local guidance.
    SaveGuidance(WorkbenchGuidanceSave),
    /// Revises one exact active guidance record.
    ReviseGuidance(WorkbenchGuidanceRevision),
    /// Changes deterministic retrieval priority for one exact guidance record.
    PinGuidance(WorkbenchGuidancePin),
    /// Changes one guidance record between project and selected-conversation scope.
    ScopeGuidance(WorkbenchGuidanceScopeChange),
    /// Excludes one exact guidance record from future retrieval using a durable tombstone.
    ForgetGuidance(WorkbenchGuidanceForget),
    /// Applies only the exact initialization file diff that the user inspected.
    ApplyInitDiff(InitProposal),
}
