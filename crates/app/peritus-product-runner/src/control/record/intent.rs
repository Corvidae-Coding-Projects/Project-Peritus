//! Closed durable control intents.

use serde::Deserialize;
use serde::Serialize;

/// Closed durable intents. Inspection is intentionally not a mutation variant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ControlIntent {
    /// Creates a non-running conversation; it grants no execution authority.
    CreateConversation {
        /// User-selected title, not a provider-generated instruction.
        title: crate::control::ControlText<256>,
    },
    /// Revises only the library title.
    RenameConversation {
        /// Exact replacement title.
        title: crate::control::ControlText<256>,
    },
    /// Revises library pinning without affecting execution or retention of required evidence.
    PinConversation {
        /// Desired library pin state.
        pinned: bool,
    },
    /// Revises reversible library visibility; archive never cancels or starts work.
    ArchiveConversation {
        /// Desired archive state.
        archived: bool,
    },
    /// Atomically reserves a child branch and its governing budget slice on the source.
    ReserveFork {
        /// Exact immutable parent/checkpoint/child binding.
        branch: crate::control::ConversationBranch,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Creates the independent non-running child side of an accepted fork.
    CreateFork {
        /// The same exact branch binding committed on the source aggregate.
        branch: crate::control::ConversationBranch,
    },
    /// Publishes an already-reserved logical branch after exact restore settlement. Host-only.
    PublishRestoreBranch {
        /// Successfully settled source restore.
        restore: crate::control::RestoreId,
        /// Exact immutable branch retained by that restore preparation.
        branch: crate::control::ConversationBranch,
    },
    /// Applies an immutable queue transition; host incorporation is not a client-granted effect.
    Queue(crate::control::QueueIntent),
    /// Revises a user-confirmed brief field through immutable input edit/correction semantics.
    SetBrief {
        /// Explicit field selected by the user.
        field: crate::control::BriefField,
        /// Exact user-confirmed replacement; no model suggestion is promoted implicitly.
        text: crate::control::ControlText<8192>,
    },
    /// Changes a user-managed context preference without rewriting source history.
    SetContext {
        /// Exact source selected from a revision-fenced context inspection.
        target: crate::control::ContextTarget,
        /// Pin/exclude preference, or none to restore ordinary host selection.
        preference: Option<crate::control::ContextPreference>,
    },
    /// Publishes a deterministic C6-validated prompt view. Host-only.
    ApplyPromptView(crate::control::PromptView),
    /// Admits one separately fenced execution lineage over the existing durable input queue.
    StartExecution {
        /// Exact execution identity, distinct from the conversation identity.
        run: [u8; 16],
        /// Canonical provider/mode/model selection; this grants no additional tool authority.
        settings_digest: [u8; 32],
    },
    /// Confirms and starts one bounded persistent goal over the existing runner.
    StartGoal {
        /// Existing product-run identity; retries retain this identity.
        run: [u8; 16],
        /// Canonical provider/mode/model selection.
        settings_digest: [u8; 32],
        /// Exact objective already present in the user-confirmed brief.
        objective: crate::control::ControlText<8192>,
        /// Typed completion criteria; unsupported kinds remain visibly unavailable.
        criteria: Vec<crate::control::GoalCriterion>,
        /// Optional cumulative limits within host ceilings.
        budget: crate::control::GoalBudget,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Requests a durable safe-boundary pause of the current goal.
    PauseGoal {
        /// Exact original goal/start operation.
        goal: crate::control::OperationId,
        /// Requested boundary semantics.
        mode: crate::control::GoalPauseMode,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Explicitly resumes the same logical goal without resetting accounting.
    ResumeGoal {
        /// Exact original goal/start operation.
        goal: crate::control::OperationId,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Replaces optional user limits without implicitly resuming work.
    UpdateGoalBudget {
        /// Exact original goal/start operation.
        goal: crate::control::OperationId,
        /// Complete replacement set of optional user limits.
        budget: crate::control::GoalBudget,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Cancels future continuation while preserving goal history and completed effects.
    ClearGoal {
        /// Exact original goal/start operation.
        goal: crate::control::OperationId,
        /// Host-observed acceptance timestamp.
        now_unix_millis: u64,
    },
    /// Reserves one provider request before D0 sends it. Host-only.
    ReserveGoalRequest {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Locally derived role attribution.
        role: crate::control::GoalRole,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Reconciles a terminal provider boundary and exact optional usage. Host-only.
    CompleteGoalRequest {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Locally derived role attribution.
        role: crate::control::GoalRole,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Provider-reported/derived fields with explicit absence.
        report: crate::control::GoalUsageReport,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Reserves one tool operation before any tool effect. Host-only.
    ReserveGoalTool {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Locally derived role attribution.
        role: crate::control::GoalRole,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Conservative host classification used by `before-edit` pause.
        mutation_capable: bool,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Marks a reserved tool operation settled at a safe boundary. Host-only.
    CompleteGoalTool {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Reconciles one attempt's monotonic runner/resource high-water marks. Host-only.
    ObserveGoalProgress {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Runner-observed active elapsed time for this attempt.
        elapsed_millis: u64,
        /// Runner retry high-water mark for this attempt.
        retries: u32,
        /// Runner provider-failover high-water mark for this attempt.
        provider_failovers: u32,
        /// Runner compaction high-water mark for this attempt.
        compactions: u32,
        /// Latest workspace bytes.
        workspace_bytes: u64,
        /// Attempt workspace-growth high-water mark.
        workspace_growth_bytes: u64,
        /// Attempt RSS high-water mark.
        peak_rss_bytes: u64,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Publishes runner settlement facts without accepting provider-authored success text. Host-only.
    SettleGoal {
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Typed terminal disposition from the runner/settlement system.
        settlement: crate::control::GoalSettlement,
        /// Exact incorporated governing-input revision for accepted evidence.
        evidence_input_generation: Option<u64>,
        /// Whether an ambiguous/unresolved effect still prevents completion.
        unresolved_effects: bool,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Publishes daemon-verified native graphical evidence. Host-only.
    ObserveGraphicalGoalEvidence {
        /// Exact zero-based criterion in the bound goal definition.
        criterion_index: u32,
        /// Exact goal binding.
        goal: crate::control::OperationId,
        /// Current one-based goal attempt.
        attempt: u32,
        /// Current user-command revision of the goal definition and limits.
        evidence_user_revision: u64,
        /// Exact governing-input revision for the inspected candidate.
        evidence_input_generation: u64,
        /// Owned preview launch carrying the inspected process evidence.
        launch: crate::control::OperationId,
        /// Published selected-window capture carrying the pixel evidence.
        capture: crate::control::OperationId,
        /// Host-observed boundary timestamp.
        now_unix_millis: u64,
    },
    /// Publishes an exact public reply artifact after a prepared invocation. Host-only.
    PublishReply(crate::control::PublicReplyReference),
    /// Publishes validated immutable image bytes and enqueues the explicit caption. Host-only.
    AttachImage {
        /// Exact retained import reference; the host must install its validated bytes atomically.
        image: crate::control::ImageAttachment,
        /// Exact user-approved caption supplied with the image.
        text: crate::control::ControlText<8192>,
    },
    /// Changes future image inclusion without deleting history or claiming past withdrawal.
    SelectImage {
        /// Original immutable image import operation.
        attachment: crate::control::OperationId,
        /// Explicit selection preference for subsequent requests.
        selected: bool,
    },
    /// Publishes a confirmed immutable file snapshot and its caption. Host-only.
    AttachFile {
        /// Checked source/reference metadata; exact bytes and consent must be installed atomically.
        file: crate::control::FileAttachment,
        /// User-confirmed caption, governed by the ordinary queue lifecycle.
        text: crate::control::ControlText<8192>,
    },
    /// Revises future file inclusion without modifying any historical request.
    SelectFile {
        /// Original reference operation, independent of refresh versions.
        attachment: crate::control::OperationId,
        /// Exact desired selection preference.
        selected: bool,
    },
    /// Appends an authorized observation before request admission. Host-only.
    RefreshFile {
        /// Original reference operation.
        attachment: crate::control::OperationId,
        /// Exact current version that the new observation supersedes.
        previous: crate::control::OperationId,
        /// New immutable version and admission-proof binding.
        version: crate::control::FileVersion,
    },
    /// Adds one typed digest-bound change-review comment and an ordinary queued user input.
    AddReview {
        /// Exact file or hunk identity selected from the current structured diff.
        anchor: crate::control::ReviewAnchor,
        /// Closed user meaning; only request-revision admits mutation work.
        feedback: crate::control::ReviewFeedback,
        /// Exact user-authored feedback text.
        message: crate::control::ControlText<8192>,
    },
    /// Rebinds a stale comment to an exact freshly inspected target.
    RebindReview {
        /// Stable original add-operation identity.
        comment: crate::control::OperationId,
        /// New exact target explicitly selected by the user.
        anchor: crate::control::ReviewAnchor,
    },
    /// Explicitly dismisses one review comment or hard constraint.
    DismissReview {
        /// Stable original add-operation identity.
        comment: crate::control::OperationId,
    },
    /// Publishes one bounded covered-path checkpoint; exact before-images are journal artifacts.
    CreateCheckpoint(crate::control::UserCheckpoint),
    /// Seals expected post-change versions at a completed owned execution boundary. Host-only.
    SealCheckpoint {
        /// Checkpoint whose coverage was active for the owned execution.
        checkpoint: crate::control::CheckpointId,
        /// Completed owned run identity; this is lineage, not a process handle.
        run: [u8; 16],
        /// Canonical path/version pairs for every covered path.
        versions: Vec<(String, crate::control::CheckpointFileVersion)>,
    },
    /// Durably prepares rewind with an exact current recovery checkpoint before file effects.
    PrepareRestore {
        /// Preview-bound operation record.
        restore: crate::control::RestoreOperation,
        /// Exact current covered versions retained before applying restore.
        recovery: crate::control::UserCheckpoint,
    },
    /// Settles an already prepared restore after the filesystem transaction returns. Host-only.
    SettleRestore {
        /// Prepared restore identity.
        restore: crate::control::RestoreId,
        /// Truthful terminal state.
        status: crate::control::RestoreStatus,
        /// Exact conflicting covered paths; empty for applied or recovery-required states.
        conflicts: Vec<String>,
        /// Exact terminal transaction evidence digest. Applied restores always retain evidence;
        /// crash-reconciled non-success states may retain a distinct recovery record.
        transaction_manifest_digest: Option<[u8; 32]>,
    },
    /// Changes only the workspace-scoped user restriction overlay. Lower authority remains required.
    SetPermissions {
        /// Exact policy revision inspected by the user.
        expected_policy_revision: u64,
        /// Capability selected by the user.
        capability: crate::control::PermissionCapability,
        /// Desired overlay value; `true` cannot exceed immutable host policy.
        allowed: bool,
    },
    /// Advances conversation fencing while atomically publishing an exact project-guidance sidecar.
    /// The inert audit text preserves the user-approved mutation in immutable C0 history.
    UpdateGuidance(crate::control::GuidanceAudit),
    /// Records an initialization patch already applied by the exact committed C1 authority.
    /// The daemon atomically retains the installed transaction manifest with this audit event.
    RecordInitialization {
        /// Canonical fingerprint of every fact in the user-confirmed proposal.
        proposal_digest: [u8; 32],
        /// Exact applied patch identity.
        patch_digest: [u8; 32],
        /// Digest of the retained installed C1 transaction manifest.
        transaction_manifest_digest: [u8; 32],
    },
}
