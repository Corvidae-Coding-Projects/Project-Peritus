//! Verus-facing ordinary-safe API shape for the daemon composition boundary.
//!
//! The real provider, filesystem, process, and Git effects are compiled from `execution` for
//! ordinary builds. Verus checks this total API shape while the ordinary API audit constrains the
//! corresponding production implementation.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{RunId, WorkspaceId};

use crate::{ProductRunnerError, control::HostPermissions};

mod command_runtime;
mod effect_stubs;
pub use command_runtime::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan, PreviewTerminal,
};
pub use effect_stubs::{
    UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect, checked_protected_file,
    uncertain_effects,
};

pub use crate::accounting::{
    ProductRunProgress, ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
    ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
};

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

/// Exact workspace object kind presented to a host checkpoint boundary before mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceMutationKind {
    /// A regular file will be created, replaced, or removed.
    File,
    /// An already-verified empty directory will be removed.
    EmptyDirectory,
}

/// Durable checkpoint preflight that can wait without releasing the pending tool context.
pub type WorkspaceCheckpointFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// Live daemon-owned conversation supplied to model turns.
pub trait ConversationView: Send + Sync {
    /// Whether media is supplied only through the revisioned input port.
    fn uses_explicit_media(&self) -> bool {
        false
    }
    /// Monotonic revision incremented whenever the user adds context.
    fn revision(&self) -> u64;
    /// Latest revision actually incorporated by a model, not merely received.
    fn incorporated_revision(&self) -> u64 {
        self.revision()
    }
    /// Human-readable chronological transcript for the next model turn.
    fn render(&self) -> String;
    /// Stable context safe to copy into a role's fixed prompt.
    fn stable_request_context(&self) -> String {
        self.render()
    }
    /// Exact user-authored text allowed to grant read-only access to explicitly named external
    /// references. Governed hosts must exclude provider replies, tool output, and host guidance.
    fn reference_authority_context(&self) -> String {
        self.render()
    }
    /// Returns one bounded page of daemon-owned immutable source descriptors after an ordinal.
    /// Source bodies remain outside the prompt and target workspace until explicitly range-read.
    fn context_sources(
        &self,
        _after: Option<u64>,
    ) -> Result<crate::ContextSourcePage, String> {
        Err("no external context sources are bound to this run".to_owned())
    }
    /// Reads one exact bounded UTF-8 slice from a descriptor returned by `context_sources`.
    /// Hosts must revalidate run scope, source digest, total length, ordinal, and chunk boundary.
    fn read_context_source(
        &self,
        _source: u64,
        _offset: u64,
    ) -> Result<crate::ContextSourceSlice, String> {
        Err("no external context sources are bound to this run".to_owned())
    }
    #[cfg(not(verus_only))]
    /// Optional host publication port for immutable product-finding bodies.
    fn finding_body_publisher(
        &self,
    ) -> Option<&dyn peritus_review::ProductFindingBodyPublisher> {
        None
    }
    /// Atomically adopts a compact ledger after all of its body references are durable.
    fn adopt_finding_state(&self, _finding_state: &str) -> Result<(), String> {
        Ok(())
    }
    /// Returns one bounded page of immutable typed governing-conversation bodies.
    fn request_sources(
        &self,
        _after: Option<u64>,
    ) -> Result<crate::ContextSourcePage, String> {
        Err("no authoritative request sources are bound to this run".to_owned())
    }
    /// Whether out-of-line user-authority bodies must be read before any workspace tool.
    fn request_sources_required(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// Exact conversation revision represented by the immutable request-source catalog.
    fn request_source_revision(&self) -> Result<u64, String> {
        Ok(self.revision())
    }
    /// Exact stable identity of the governing user-source set.
    fn request_source_binding(&self) -> [u8; 32] {
        let mut binding = [0_u8; 32];
        binding[24..].copy_from_slice(&self.revision().to_be_bytes());
        binding
    }
    /// Exact identity of the complete typed source catalog.
    fn request_source_catalog_binding(&self) -> [u8; 32] {
        self.request_source_binding()
    }
    /// Reads one exact bounded UTF-8 slice from a descriptor returned by `request_sources`.
    fn read_request_source(
        &self,
        _source: u64,
        _offset: u64,
    ) -> Result<crate::ContextSourceSlice, String> {
        Err("no authoritative request sources are bound to this run".to_owned())
    }
    /// Current hard relative paths narrowed by explicit leave-alone review constraints.
    fn protected_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Latest host-intersected execution capabilities. Legacy embedders retain their prior
    /// behavior; governed hosts override this with a live, fail-closed durable snapshot.
    fn effective_permissions(&self) -> HostPermissions {
        HostPermissions::all()
    }
    /// Whether the currently pending typed review feedback permits an implementation handoff.
    fn permits_pipeline_handoff(&self) -> bool {
        true
    }
    /// Durably captures one exact workspace-relative target before its first owned mutation.
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
    /// Awaitable checkpoint preflight for recoverable resource shortages.
    ///
    /// # Errors
    /// Returns an error for cancellation, changed authority or an unrecoverable checkpoint.
    fn checkpoint_before_workspace_mutation_async<'a>(
        &'a self,
        relative_path: &'a Path,
        kind: WorkspaceMutationKind,
    ) -> WorkspaceCheckpointFuture<'a> {
        Box::pin(async move { self.checkpoint_before_workspace_mutation(relative_path, kind) })
    }
    /// Publishes an exact before-image retained before a command after its changed path is known.
    ///
    /// # Errors
    /// Returns a redaction-safe reason when the retained baseline cannot be published durably.
    fn checkpoint_workspace_mutation_from_baseline(
        &self,
        _relative_path: &Path,
        _kind: WorkspaceMutationKind,
        _baseline: &crate::control::WorkspaceMutationBaseline,
    ) -> Result<(), String> {
        Ok(())
    }
    /// Durably seals a checkpoint with the exact postimage produced by the owned tool.
    /// Hosts without rewind support retain the same optional callback as ordinary builds.
    ///
    /// # Errors
    /// Returns a redaction-safe reason when the owned postimage cannot be durably recorded.
    fn seal_workspace_mutation_checkpoint(
        &self,
        _relative_path: &Path,
        _kind: WorkspaceMutationKind,
        _owned_postchange: crate::control::CheckpointFileVersion,
    ) -> Result<(), String> {
        Ok(())
    }
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
pub struct ProductRunInput {
    /// Caller-resolved managed or in-place delivery boundary.
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
    pub command_runtime: CommandRuntime,
    /// Durable D2 finding ledger restored by the daemon.
    pub finding_state: String,
    /// Natural-language coding task.
    pub task: String,
    /// Optional caller-selected wall-clock horizon. `None` permits uninterrupted execution.
    pub max_elapsed: Option<Duration>,
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

    /// Verification-only state never claims reusable production gate evidence.
    #[must_use]
    pub const fn retains_current_gate_state(&self) -> bool {
        false
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
            crate::ProductRunnerErrorKind::InvalidPrecondition,
            "execute verification-only product runner",
            "production effects are unavailable in a verus_only build",
        ))
    }
}
