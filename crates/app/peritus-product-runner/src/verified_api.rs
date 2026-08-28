//! Verus-facing ordinary-safe API shape for the daemon composition boundary.
//!
//! The real provider, filesystem, process, and Git effects are compiled from `execution` for
//! ordinary builds. Verus checks this total API shape while the ordinary API audit constrains the
//! corresponding production implementation.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

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
