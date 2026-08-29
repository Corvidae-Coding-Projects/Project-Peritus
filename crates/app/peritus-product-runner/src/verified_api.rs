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

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

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

/// Fully resolved daemon input for one product run.
pub struct ProductRunInput {
    /// Stable run identity.
    pub run_id: RunId,
    /// Canonical managed-worktree root.
    pub workspace_root: PathBuf,
    /// Durable D0 trace path owned by the daemon.
    pub trace_path: PathBuf,
    /// Durable D2 finding ledger restored by the daemon.
    pub finding_state: String,
    /// Natural-language coding task.
    pub task: String,
    /// Live conversation, including the original task and all follow-ups.
    pub conversation: Arc<dyn ConversationView>,
    /// Role provider adapters.
    pub providers: RoleProviders,
    /// Shared cancellation state.
    pub cancelled: Arc<AtomicBool>,
    /// Provider cancellation token.
    pub provider_cancellation: CancellationToken,
}

/// Exact deliverable evidence from a successful production run.
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

/// Terminal product-run result.
pub enum ProductRunOutcome {
    /// Passing implementation and review evidence.
    Complete(ProductRunOutput),
    /// The writer cannot proceed without one material user choice.
    WaitingForUser {
        /// Direct question to present in the run conversation.
        question: String,
        /// Conversation revision on which the question was based.
        conversation_revision: u64,
    },
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
            ProductRunnerErrorKind::Gate,
            "execute verification-only product runner",
            "production effects are unavailable in a verus_only build",
        ))
    }
}
