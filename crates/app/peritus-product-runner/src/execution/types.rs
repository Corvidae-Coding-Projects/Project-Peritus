//! Public run contracts and internal completed-turn values.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::RunId;

use crate::ProductRunProgress;

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

/// One progress observation emitted at a completed effect boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Observer invoked synchronously after each daemon-visible boundary.
pub type RunObserver = Arc<dyn Fn(ProductRunUpdate) + Send + Sync>;

/// Live daemon-owned conversation supplied to every model turn.
pub trait ConversationView: Send + Sync {
    /// Monotonic revision incremented whenever the user adds context.
    fn revision(&self) -> u64;
    /// Human-readable chronological transcript for the next model turn.
    fn render(&self) -> String;
}

/// Explicit writer, reviewer, and fixer provider instances.
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

/// Fully resolved input supplied by the daemon authority boundary.
pub struct ProductRunInput {
    /// Stable run identity.
    pub run_id: RunId,
    /// Canonical managed-worktree root.
    pub workspace_root: PathBuf,
    /// Durable D0 trace path owned by the daemon.
    pub trace_path: PathBuf,
    /// Run-owned command router backed by the daemon's durable process registry.
    pub command_runtime: crate::CommandRuntime,
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
}

/// Successful terminal result and exact deliverable evidence.
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

impl ProductRunOutput {
    /// Number of exact candidate files.
    #[must_use]
    pub const fn changed_files(&self) -> usize {
        self.changed_paths.len()
    }
}

/// A completed run or a material question that needs a user reply.
#[derive(Clone, Debug, Eq, PartialEq)]
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
