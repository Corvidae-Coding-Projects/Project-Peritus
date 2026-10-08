//! Cooperative progress and cancellation for synchronous deterministic analysis.

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerJobId, DebuggerOperation, DebuggerRecovery,
    SelectionCounts, SelectionManifestId, TraceSelectionManifest,
};
use peritus_types::Sha256Digest;

/// Stable deterministic analysis stage reported at cooperative checkpoints.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AnalysisStage {
    /// Constructing canonical per-subject timelines.
    BuildTimelines,
    /// Running the frozen causal analyzer registry.
    AnalyzeCauses,
    /// Grouping findings into canonical patterns.
    ClusterPatterns,
    /// Mapping patterns to exact E1 declarations.
    MapComponents,
    /// Checking report structure and citations.
    ValidateReport,
    /// Streaming canonical report bytes into durable pages.
    EncodeReport,
}

/// Exact immutable context retained across a cooperatively suspended analysis call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisContext {
    job_id: Option<DebuggerJobId>,
    manifest_id: SelectionManifestId,
    manifest_digest: Sha256Digest,
    query_digest: Sha256Digest,
    native_context_digest: Option<Sha256Digest>,
    accepted_evidence: SelectionCounts,
}

impl AnalysisContext {
    /// Binds work to the accepted manifest without a durable job owner.
    #[must_use]
    pub const fn for_manifest(manifest: &TraceSelectionManifest) -> Self {
        Self {
            job_id: None,
            manifest_id: manifest.id(),
            manifest_digest: manifest.digest(),
            query_digest: manifest.query_digest(),
            native_context_digest: None,
            accepted_evidence: manifest.counts(),
        }
    }

    /// Binds work to its durable job and caller-defined native execution context.
    #[must_use]
    pub const fn for_job(
        job_id: DebuggerJobId,
        manifest: &TraceSelectionManifest,
        native_context_digest: Sha256Digest,
    ) -> Self {
        Self {
            job_id: Some(job_id),
            manifest_id: manifest.id(),
            manifest_digest: manifest.digest(),
            query_digest: manifest.query_digest(),
            native_context_digest: Some(native_context_digest),
            accepted_evidence: manifest.counts(),
        }
    }

    /// Returns the durable job owner when supplied.
    #[must_use]
    pub const fn job_id(self) -> Option<DebuggerJobId> {
        self.job_id
    }
    /// Returns the exact accepted manifest identity.
    #[must_use]
    pub const fn manifest_id(self) -> SelectionManifestId {
        self.manifest_id
    }
    /// Returns the complete accepted manifest digest.
    #[must_use]
    pub const fn manifest_digest(self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Returns the frozen query digest.
    #[must_use]
    pub const fn query_digest(self) -> Sha256Digest {
        self.query_digest
    }
    /// Returns the caller-defined native execution-context digest when supplied.
    #[must_use]
    pub const fn native_context_digest(self) -> Option<Sha256Digest> {
        self.native_context_digest
    }
    /// Returns the exact accepted evidence inventory.
    #[must_use]
    pub const fn accepted_evidence(self) -> SelectionCounts {
        self.accepted_evidence
    }

    pub(crate) fn validate(
        self,
        manifest: &TraceSelectionManifest,
        operation: DebuggerOperation,
    ) -> Result<(), DebuggerError> {
        if self.manifest_id == manifest.id()
            && self.manifest_digest == manifest.digest()
            && self.query_digest == manifest.query_digest()
            && self.accepted_evidence == manifest.counts()
        {
            Ok(())
        } else {
            Err(DebuggerError::new(
                DebuggerErrorKind::Binding,
                operation,
                DebuggerRecovery::CorrectInput,
                "analysis context differs from the accepted evidence manifest",
            ))
        }
    }
}

/// One monotonic progress observation within an exact analysis context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisProgress {
    context: AnalysisContext,
    stage: AnalysisStage,
    completed: u64,
    total: u64,
}

impl AnalysisProgress {
    /// Returns the exact job, manifest, query, native-context, and evidence binding.
    #[must_use]
    pub const fn context(self) -> AnalysisContext {
        self.context
    }
    /// Returns the current deterministic stage.
    #[must_use]
    pub const fn stage(self) -> AnalysisStage {
        self.stage
    }
    /// Returns completed work units.
    #[must_use]
    pub const fn completed(self) -> u64 {
        self.completed
    }
    /// Returns total work units for this stage.
    #[must_use]
    pub const fn total(self) -> u64 {
        self.total
    }
}

/// Decision returned by a synchronous cooperative checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisControlAction {
    /// Continue from the exact retained native stack and accepted inputs.
    Continue,
    /// Stop at this deterministic boundary without emitting partial analysis output.
    Cancel,
}

/// Receives progress and controls cooperative suspension or cancellation.
///
/// Implementations may block inside `checkpoint` and later return `Continue`; this suspends and
/// resumes the same native call stack without reconstructing context or accepting replacement
/// evidence.
pub trait AnalysisControl {
    /// Observes one deterministic boundary and decides whether work continues.
    fn checkpoint(&mut self, progress: AnalysisProgress) -> AnalysisControlAction;
}

/// Controller used by compatibility entry points that run to completion.
#[derive(Default)]
pub(crate) struct RunToCompletion;

impl AnalysisControl for RunToCompletion {
    fn checkpoint(&mut self, _progress: AnalysisProgress) -> AnalysisControlAction {
        AnalysisControlAction::Continue
    }
}

pub(crate) fn checkpoint(
    control: &mut impl AnalysisControl,
    context: AnalysisContext,
    stage: AnalysisStage,
    completed: usize,
    total: usize,
    operation: DebuggerOperation,
) -> Result<(), DebuggerError> {
    let progress = AnalysisProgress {
        context,
        stage,
        completed: u64::try_from(completed).unwrap_or(u64::MAX),
        total: u64::try_from(total).unwrap_or(u64::MAX),
    };
    match control.checkpoint(progress) {
        AnalysisControlAction::Continue => Ok(()),
        AnalysisControlAction::Cancel => Err(DebuggerError::new(
            DebuggerErrorKind::Cancelled,
            operation,
            DebuggerRecovery::None,
            "deterministic analysis was cooperatively cancelled",
        )),
    }
}
