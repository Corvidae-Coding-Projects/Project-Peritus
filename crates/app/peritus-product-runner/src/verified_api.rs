//! Verus-facing ordinary-safe API shape for the daemon composition boundary.
//!
//! The real provider, filesystem, process, and Git effects are compiled from `execution` for
//! ordinary builds. Verus checks this total API shape while the ordinary API audit constrains the
//! corresponding production implementation.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use peritus_process::ProcessStore;
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{RunId, WorkspaceId};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Maximum wall-clock duration of one uninterrupted product-run attempt.
pub const PRODUCT_RUN_MAX_ELAPSED: Duration = Duration::from_hours(8);
/// Maximum provider requests across all product roles.
pub const PRODUCT_RUN_MAX_MODEL_REQUESTS: u32 = 4_096;
/// Maximum application tool calls across all product roles.
pub const PRODUCT_RUN_MAX_TOOL_CALLS: u32 = 20_000;
/// Maximum aggregate provider tokens across all product roles.
pub const PRODUCT_RUN_MAX_TOTAL_TOKENS: u64 = 100_000_000;
/// Maximum provider-estimated cost in integer microunits.
pub const PRODUCT_RUN_MAX_COST_MICROUNITS: u64 = 500_000_000;
/// Maximum observed resident memory at a completed effect boundary.
pub const PRODUCT_RUN_MAX_PEAK_RSS_BYTES: u64 = 12 * 1024 * 1024 * 1024;
/// Maximum regular-file growth beneath the managed workspace during one run.
pub const PRODUCT_RUN_MAX_WORKSPACE_GROWTH_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Monotonic aggregate progress for one complete product-run attempt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProductRunProgress {
    model_requests: u32,
    tool_calls: u32,
    retries: u32,
    provider_failovers: u32,
    compactions: u32,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    provider_cost_microunits: u64,
    usage_observations: u32,
    elapsed_millis: u64,
    workspace_bytes: u64,
    workspace_growth_bytes: u64,
    peak_rss_bytes: u64,
}

impl ProductRunProgress {
    /// Provider requests completed or terminally observed.
    pub const fn model_requests(self) -> u32 {
        self.model_requests
    }
    /// Application tool calls completed.
    pub const fn tool_calls(self) -> u32 {
        self.tool_calls
    }
    /// Checked provider retries completed.
    pub const fn retries(self) -> u32 {
        self.retries
    }
    /// Explicit switches to another configured provider.
    pub const fn provider_failovers(self) -> u32 {
        self.provider_failovers
    }
    /// Deterministic context compactions applied.
    pub const fn compactions(self) -> u32 {
        self.compactions
    }
    /// Provider-reported input tokens.
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }
    /// Provider-reported cache-read input tokens.
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }
    /// Provider-reported output tokens.
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }
    /// Explicit or conservatively derived aggregate tokens.
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
    /// Provider-estimated cost in integer microunits.
    pub const fn provider_cost_microunits(self) -> u64 {
        self.provider_cost_microunits
    }
    /// Responses that supplied normalized usage.
    pub const fn usage_observations(self) -> u32 {
        self.usage_observations
    }
    /// Elapsed milliseconds at the latest effect boundary.
    pub const fn elapsed_millis(self) -> u64 {
        self.elapsed_millis
    }
    /// Current regular-file bytes beneath the workspace, excluding Git object storage.
    pub const fn workspace_bytes(self) -> u64 {
        self.workspace_bytes
    }
    /// Positive workspace growth since this product-run attempt began.
    pub const fn workspace_growth_bytes(self) -> u64 {
        self.workspace_growth_bytes
    }
    /// Highest resident-memory observation at a completed effect boundary.
    pub const fn peak_rss_bytes(self) -> u64 {
        self.peak_rss_bytes
    }
}

/// Concrete product-run phase emitted to the daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductRunPhase {
    /// Repository inspection and detailed implementation design.
    Designing,
    /// Writer model and developer tools.
    Writing,
    /// Exact-target repository checks.
    Checking,
    /// Independent typed review.
    Reviewing,
    /// Finding-conserving fixer loop.
    Fixing,
    /// Fresh exact-target checks after a fix.
    Verifying,
    /// Final candidate refresh, evidence classification, and handoff construction.
    Finalizing,
    /// Passing terminal state.
    Complete,
}

/// One daemon-visible progress observation.
pub struct ProductRunUpdate {
    /// Current phase.
    pub phase: ProductRunPhase,
    /// One-based implementation cycle.
    pub cycle: u32,
    /// Current operation in user language.
    pub status: String,
    /// Current bounded diff.
    pub diff: String,
    /// Latest exact-target gate output.
    pub gates: String,
    /// Latest conserved typed review output.
    pub review: String,
    /// Interim task-level summary.
    pub summary: String,
    /// Durable typed finding ledger at this effect boundary.
    pub finding_state: String,
    /// Cumulative resource accounting at this completed effect boundary.
    pub progress: ProductRunProgress,
    /// Strongest exact candidate checkpoint observed at this boundary.
    pub checkpoint: Option<CandidateCheckpoint>,
    /// Concrete phases or evidence still needed for strict acceptance.
    pub remaining_work: Vec<String>,
}

/// Synchronous observer for a completed effect boundary.
pub type RunObserver = Arc<dyn Fn(ProductRunUpdate) + Send + Sync>;

/// Live daemon-owned conversation supplied to model turns.
pub trait ConversationView: Send + Sync {
    /// Monotonic revision incremented whenever the user adds context.
    fn revision(&self) -> u64;
    /// Human-readable chronological transcript for the next model turn.
    fn render(&self) -> String;
}

/// Explicit provider instances for the three orchestration roles.
pub struct RoleProviders {
    /// Writer model adapter.
    pub writer: Arc<dyn ModelProvider>,
    /// Independent reviewer adapter.
    pub reviewer: Arc<dyn ModelProvider>,
    /// Fixer model adapter.
    pub fixer: Arc<dyn ModelProvider>,
    /// User-authorized fallback adapters considered after a selected provider exhausts recovery.
    pub fallbacks: Vec<Arc<dyn ModelProvider>>,
}

/// Authorized form of deliverable evidence for one product run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductDeliveryScope {
    /// Normal coding work must produce exact changed workspace paths and pass their gates.
    WorkspaceChanges,
    /// The caller authorizes a deliverable implemented as durable effects outside the workspace.
    AuthorizedExternalEffects,
}

impl ProductDeliveryScope {
    /// Whether an empty workspace candidate may be evaluated from external-effect evidence.
    #[must_use]
    pub const fn allows_external_effects(self) -> bool {
        matches!(self, Self::AuthorizedExternalEffects)
    }
}

/// Fully resolved daemon input for one product run.
#[derive(Clone, Debug)]
pub struct CommandRuntime;

impl CommandRuntime {
    /// Preserves the ordinary command-runtime construction boundary in the Verus API model.
    ///
    /// The effectful implementation validates the roots and constructs the C4/C2 runtime. The
    /// verified API carries the already-resolved value across the daemon composition boundary.
    ///
    /// # Errors
    ///
    /// The ordinary implementation reports invalid roots or runtime construction failures.
    pub fn open(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, ProductRunnerError> {
        let _ = (state_root.into(), workspace_root.into(), run_id, process_store);
        Ok(Self)
    }
}

/// Fully resolved daemon input for one product run.
pub struct ProductRunInput {
    /// Stable run identity.
    pub run_id: RunId,
    /// Stable managed-workspace lineage supplied by the daemon authority boundary.
    pub workspace_id: WorkspaceId,
    /// Canonical managed-worktree root.
    pub workspace_root: PathBuf,
    /// Durable D0 trace path owned by the daemon.
    pub trace_path: PathBuf,
    /// Run-owned command router backed by the daemon's durable process registry.
    pub command_runtime: CommandRuntime,
    /// Durable D2 finding ledger restored by the daemon.
    pub finding_state: String,
    /// Natural-language coding task.
    pub task: String,
    /// Caller-resolved wall-clock horizon, bounded by the product's eight-hour hard ceiling.
    pub max_elapsed: Duration,
    /// Caller-authorized deliverable boundary. Ordinary product runs use workspace changes.
    pub delivery_scope: ProductDeliveryScope,
    /// Live conversation, including the original task and all follow-ups.
    pub conversation: Arc<dyn ConversationView>,
    /// Role provider adapters.
    pub providers: RoleProviders,
    /// Shared cancellation state.
    pub cancelled: Arc<AtomicBool>,
    /// Provider cancellation token.
    pub provider_cancellation: CancellationToken,
    /// Prior S5 handoff to validate and resume without repeating current phases.
    pub resume: Option<ProductRunResume>,
}

/// Exact deliverable evidence from a successful production run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOutput {
    /// Durable detailed implementation design generated before coding.
    pub design_path: PathBuf,
    /// Aggregated task-level completion summary.
    pub summary: String,
    /// Final bounded diff.
    pub diff: String,
    /// Final passing exact-target gate output.
    pub gates: String,
    /// Final conserved review ledger.
    pub review: String,
    /// Exact task candidate paths.
    pub changed_paths: Vec<PathBuf>,
    /// Exact successful acceptance commands.
    pub successful_commands: Vec<String>,
    /// Exact command or concise steps for running the accepted deliverable.
    pub run_instructions: String,
    /// Number of fixer cycles used.
    pub fixer_cycles: u32,
    /// Conversation revision incorporated by the accepted implementation.
    pub conversation_revision: u64,
}

/// Opaque digest-bound continuation state for an interrupted product run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunResume {
    checkpoint: CandidateCheckpoint,
    next_phase: ProductRunPhase,
}

impl ProductRunResume {
    /// Exact candidate checkpoint at the interruption boundary.
    #[must_use]
    pub const fn checkpoint(&self) -> &CandidateCheckpoint {
        &self.checkpoint
    }

    /// First phase that was stale or incomplete when the run stopped.
    #[must_use]
    pub const fn next_phase(&self) -> ProductRunPhase {
        self.next_phase
    }
}

/// One material question retained alongside a waiting settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunQuestion {
    message: String,
    conversation_revision: u64,
}

impl ProductRunQuestion {
    /// Direct question to present in the run conversation.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Conversation revision on which the question was based.
    #[must_use]
    pub const fn conversation_revision(&self) -> u64 {
        self.conversation_revision
    }
}

/// Verified terminal settlement plus its exact candidate and continuation handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOutcome {
    settlement: RunSettlement,
    candidate: Option<ProductRunOutput>,
    question: Option<ProductRunQuestion>,
    detail: Option<String>,
    remaining_work: Vec<String>,
    resume: Option<ProductRunResume>,
}

impl ProductRunOutcome {
    /// Verified terminal truth for this run.
    #[must_use]
    pub const fn settlement(&self) -> &RunSettlement {
        &self.settlement
    }

    /// Strongest candidate handoff, including incomplete evidence when present.
    #[must_use]
    pub const fn candidate(&self) -> Option<&ProductRunOutput> {
        self.candidate.as_ref()
    }

    /// Material user question when the disposition is waiting.
    #[must_use]
    pub const fn question(&self) -> Option<&ProductRunQuestion> {
        self.question.as_ref()
    }

    /// Redaction-safe terminal diagnostic independent of candidate quality.
    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// Concrete phases or evidence still needed for strict acceptance.
    #[must_use]
    pub const fn remaining_work(&self) -> &[String] {
        self.remaining_work.as_slice()
    }

    /// Digest-bound continuation state when an exact candidate is available.
    #[must_use]
    pub const fn resume(&self) -> Option<&ProductRunResume> {
        self.resume.as_ref()
    }
}

/// Verus-facing runner boundary. Real effects exist only in the ordinary implementation.
pub struct ProductRunner;

impl ProductRunner {
    /// Produces no fabricated completion in a verification-only build.
    pub async fn run(
        _input: ProductRunInput,
        _observe: RunObserver,
    ) -> Result<ProductRunOutcome, ProductRunnerError> {
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidPrecondition,
            "execute verification-only product runner",
            "production effects are unavailable in a verus_only build",
        ))
    }
}
