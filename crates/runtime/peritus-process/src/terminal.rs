//! Deterministic terminal execution result.

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    EscalationRecord, OutputCompleteness, OutputStream, ProcessResourceObservation, StopTrigger,
    StreamAccounting,
};

mod canonical;
#[cfg(test)]
mod canonical_tests;

pub(crate) use canonical::{decode_terminal, encode_terminal, terminal_digest};

/// Underlying platform exit observation retained independently of top-level classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OsExitObservation {
    /// A numeric exit status was reported.
    Code(i32),
    /// A Unix signal terminated the process.
    Signal(i32),
    /// A PTY adapter reported a platform signal name without its numeric value.
    SignalName(String),
    /// A platform exception/status terminated the process.
    PlatformException(u32),
    /// No trustworthy operating-system exit was available.
    Unavailable,
}

/// Stable top-level outcome selected by the deterministic lifecycle reducer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalDisposition {
    /// The process exited with a numeric status.
    Exited,
    /// The process was signalled or raised a platform exception.
    Signalled,
    /// Process creation failed after durable authority consumption.
    SpawnFailed,
    /// Explicit cancellation won the first-trigger race.
    Cancelled,
    /// The wall deadline won the first-trigger race.
    TimedOut,
    /// An output ceiling won the first-trigger race.
    OutputLimit,
    /// Another resource ceiling won the first-trigger race.
    ResourceLimit,
    /// The sandbox denied or failed the active execution.
    SandboxDenied,
    /// The supervisor could not complete ownership duties.
    SupervisorFailed,
    /// Restart or cleanup could not establish exact terminal facts.
    RecoveryIndeterminate,
}

/// Recovery relation attached to the terminal result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TerminalRecovery {
    /// Produced in the original owning process.
    OriginalOwner,
    /// Recovered from an already terminal durable record.
    ReopenedTerminal,
    /// An exact live tree was cancelled and reaped during reconciliation.
    ReconciledLive,
    /// Recovery could not establish exact process identity or terminal state.
    Indeterminate,
}

/// Monotonic observation relative to supervisor startup.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessInstant {
    millis: u64,
}

impl ProcessInstant {
    /// Creates one relative monotonic observation.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self { millis }
    }
    /// Returns elapsed milliseconds from supervisor startup.
    #[must_use]
    pub const fn millis(self) -> u64 {
        self.millis
    }
}

/// Exact retained output summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSummary {
    streams: Vec<StreamAccounting>,
    event_records_dropped: u64,
}

impl OutputSummary {
    /// Creates a terminal output summary.
    #[must_use]
    pub const fn new(streams: Vec<StreamAccounting>, event_records_dropped: u64) -> Self {
        Self { streams, event_records_dropped }
    }
    /// Returns per-stream accounting.
    #[must_use]
    pub fn streams(&self) -> &[StreamAccounting] {
        &self.streams
    }
    /// Returns events removed from the bounded in-memory event window.
    #[must_use]
    pub const fn event_records_dropped(&self) -> u64 {
        self.event_records_dropped
    }
    /// Returns whether every stream reached exact EOF without truncation.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.streams.iter().all(|stream| stream.completeness() == OutputCompleteness::Complete)
    }
}

/// Content-addressed retained output artifact reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OutputArtifact {
    stream: OutputStream,
    digest: Sha256Digest,
    size: u64,
    start_offset: u64,
    end_offset: u64,
    completeness: OutputCompleteness,
}

impl OutputArtifact {
    /// Creates an exact stream artifact reference.
    #[must_use]
    pub const fn new(
        stream: OutputStream,
        digest: Sha256Digest,
        size: u64,
        start_offset: u64,
        end_offset: u64,
        completeness: OutputCompleteness,
    ) -> Self {
        Self { stream, digest, size, start_offset, end_offset, completeness }
    }
    /// Returns the retained stream.
    #[must_use]
    pub const fn stream(self) -> OutputStream {
        self.stream
    }
    /// Returns the artifact digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns retained bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Returns the inclusive start offset.
    #[must_use]
    pub const fn start_offset(self) -> u64 {
        self.start_offset
    }
    /// Returns the exclusive end offset.
    #[must_use]
    pub const fn end_offset(self) -> u64 {
        self.end_offset
    }
    /// Returns stream completeness at publication.
    #[must_use]
    pub const fn completeness(self) -> OutputCompleteness {
        self.completeness
    }
}

/// Complete unique terminal result for one owned execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalResult {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    disposition: TerminalDisposition,
    os_exit: OsExitObservation,
    first_trigger: Option<StopTrigger>,
    escalation: EscalationRecord,
    started_at: Option<ProcessInstant>,
    ended_at: ProcessInstant,
    output: OutputSummary,
    resources: Vec<ProcessResourceObservation>,
    artifacts: Vec<OutputArtifact>,
    tree_cleanup_complete: bool,
    support_tasks_joined: bool,
    artifact_publication_complete: bool,
    recovery: TerminalRecovery,
}

impl TerminalResult {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        process_id: ProcessId,
        plan_digest: Sha256Digest,
        disposition: TerminalDisposition,
        os_exit: OsExitObservation,
        first_trigger: Option<StopTrigger>,
        escalation: EscalationRecord,
        started_at: Option<ProcessInstant>,
        ended_at: ProcessInstant,
        output: OutputSummary,
        resources: Vec<ProcessResourceObservation>,
        tree_cleanup_complete: bool,
        support_tasks_joined: bool,
        recovery: TerminalRecovery,
    ) -> Self {
        let artifact_publication_complete =
            output.streams().iter().all(|stream| stream.retained() == 0);
        Self {
            process_id,
            plan_digest,
            disposition,
            os_exit,
            first_trigger,
            escalation,
            started_at,
            ended_at,
            output,
            resources,
            artifacts: Vec::new(),
            tree_cleanup_complete,
            support_tasks_joined,
            artifact_publication_complete,
            recovery,
        }
    }

    /// Returns the process identity.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }
    /// Returns the execution-plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the deterministic top-level disposition.
    #[must_use]
    pub const fn disposition(&self) -> TerminalDisposition {
        self.disposition
    }
    /// Returns the independent platform exit observation.
    #[must_use]
    pub const fn os_exit(&self) -> &OsExitObservation {
        &self.os_exit
    }
    /// Returns the first accepted stop trigger.
    #[must_use]
    pub const fn first_trigger(&self) -> Option<StopTrigger> {
        self.first_trigger
    }
    /// Returns graceful/forced escalation facts.
    #[must_use]
    pub const fn escalation(&self) -> EscalationRecord {
        self.escalation
    }
    /// Returns startup observation time when spawn succeeded.
    #[must_use]
    pub const fn started_at(&self) -> Option<ProcessInstant> {
        self.started_at
    }
    /// Returns terminal observation time.
    #[must_use]
    pub const fn ended_at(&self) -> ProcessInstant {
        self.ended_at
    }
    /// Returns complete output accounting.
    #[must_use]
    pub const fn output(&self) -> &OutputSummary {
        &self.output
    }
    /// Returns terminal resource observations.
    #[must_use]
    pub fn resources(&self) -> &[ProcessResourceObservation] {
        &self.resources
    }
    /// Returns finalized output artifact references.
    #[must_use]
    pub fn artifacts(&self) -> &[OutputArtifact] {
        &self.artifacts
    }
    /// Returns whether complete tree cleanup was observed.
    #[must_use]
    pub const fn tree_cleanup_complete(&self) -> bool {
        self.tree_cleanup_complete
    }
    /// Returns whether every support task joined.
    #[must_use]
    pub const fn support_tasks_joined(&self) -> bool {
        self.support_tasks_joined
    }
    /// Returns whether all requested artifact publications completed.
    #[must_use]
    pub const fn artifact_publication_complete(&self) -> bool {
        self.artifact_publication_complete
    }
    /// Returns the recovery relation.
    #[must_use]
    pub const fn recovery(&self) -> TerminalRecovery {
        self.recovery
    }

    pub(crate) fn add_artifact(&mut self, artifact: OutputArtifact) {
        self.artifacts.push(artifact);
        self.artifacts.sort_by_key(|item| match item.stream() {
            OutputStream::Stdout => 1,
            OutputStream::Stderr => 2,
            OutputStream::Terminal => 3,
        });
    }

    pub(crate) const fn mark_artifact_failure(&mut self) {
        self.artifact_publication_complete = false;
    }

    pub(crate) const fn mark_artifacts_complete(&mut self) {
        self.artifact_publication_complete = true;
    }
}
