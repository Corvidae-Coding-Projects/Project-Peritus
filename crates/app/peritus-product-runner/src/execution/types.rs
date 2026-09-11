//! Public run contracts and internal completed-turn values.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{RunId, WorkspaceId};

use crate::{
    ProductRunProgress,
    control::{CheckpointFileVersion, HostPermissions},
    execution::resume::ProductRunResume,
};

/// Stateless product-run entry point using the D0/D1/D2/E0 production composition.
pub struct ProductRunner;

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
    /// Strongest exact candidate checkpoint observed at this boundary.
    pub checkpoint: Option<CandidateCheckpoint>,
    /// Concrete phases or evidence still needed for strict acceptance.
    pub remaining_work: Vec<String>,
}

/// Observer invoked synchronously after each daemon-visible boundary.
pub type RunObserver = Arc<dyn Fn(ProductRunUpdate) + Send + Sync>;

/// Exact workspace object kind presented to a host checkpoint boundary before mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceMutationKind {
    /// A regular file will be created, replaced, or removed.
    File,
    /// An already-verified empty directory will be removed.
    EmptyDirectory,
}

/// Live daemon-owned conversation supplied to every model turn.
pub trait ConversationView: Send + Sync {
    /// Whether media is supplied only through the revisioned input port. Governed conversations
    /// must not discover or cache image bytes in immutable role scaffolding.
    fn uses_explicit_media(&self) -> bool {
        false
    }
    /// Optional daemon-owned live input and public activity port.
    fn interaction(&self) -> Option<&dyn peritus_agent::DeveloperInteraction> {
        None
    }
    /// Monotonic revision incremented whenever the user adds context.
    fn revision(&self) -> u64;
    /// Latest revision actually incorporated by a model, not merely received.
    fn incorporated_revision(&self) -> u64 {
        self.revision()
    }
    /// Human-readable chronological transcript for the next model turn.
    fn render(&self) -> String;
    /// Stable context safe to copy into a role's fixed prompt. Governed hosts exclude mutable
    /// pending inputs here and supply their current exact view through the D0 interaction port.
    fn stable_request_context(&self) -> String {
        self.render()
    }
    /// Current hard relative paths narrowed by explicit leave-alone review constraints.
    /// Implementations must fail closed when durable state cannot be read.
    fn protected_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Latest host-intersected execution capabilities. Legacy embedders retain their prior
    /// behavior; governed hosts override this with a live, fail-closed durable snapshot.
    fn effective_permissions(&self) -> HostPermissions {
        HostPermissions::all()
    }
    /// Whether the currently pending typed review feedback permits an implementation handoff.
    /// This is only a narrowing signal; ordinary user intent and every existing gate still apply.
    fn permits_pipeline_handoff(&self) -> bool {
        true
    }
    /// Durably captures one exact workspace-relative target before its first owned mutation.
    ///
    /// Hosts without user rewind checkpoints may keep the default no-op. A host that enables
    /// checkpointing must return only after the before-image is durable; an error aborts the
    /// pending workspace effect.
    ///
    /// # Errors
    /// Returns a redaction-safe reason when the host cannot publish an exact durable before-image.
    fn checkpoint_before_workspace_mutation(
        &self,
        _relative_path: &Path,
        _kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Durably seals an automatic checkpoint with the exact postimage produced by an owned tool.
    ///
    /// Hosts without user rewind checkpoints may keep the default no-op. The version is supplied
    /// by the admitted effect or its completed receipt; hosts must not replace it with a later
    /// arbitrary workspace observation.
    ///
    /// # Errors
    /// Returns a redaction-safe reason when the owned postimage cannot be durably recorded.
    fn seal_workspace_mutation_checkpoint(
        &self,
        _relative_path: &Path,
        _kind: WorkspaceMutationKind,
        _owned_postchange: CheckpointFileVersion,
    ) -> Result<(), String> {
        Ok(())
    }
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
    /// Caller-resolved delivery adapter; interaction mode cannot widen this authority.
    pub workspace_kind: crate::ProductWorkspaceKind,
    /// Stable run identity.
    pub run_id: RunId,
    /// Stable managed-workspace lineage supplied by the daemon authority boundary.
    pub workspace_id: WorkspaceId,
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
    /// Prior S5 handoff to validate and resume without repeating current phases.
    pub resume: Option<ProductRunResume>,
}

/// Strongest exact candidate handoff, whether qualified or incomplete.
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

/// One material question retained alongside a waiting settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunQuestion {
    pub(super) message: String,
    pub(super) conversation_revision: u64,
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

/// Verified terminal settlement plus its exact candidate, continuation, and diagnostic handoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunOutcome {
    pub(super) settlement: RunSettlement,
    pub(super) candidate: Option<ProductRunOutput>,
    pub(super) question: Option<ProductRunQuestion>,
    pub(super) detail: Option<String>,
    pub(super) remaining_work: Vec<String>,
    pub(super) resume: Option<ProductRunResume>,
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
