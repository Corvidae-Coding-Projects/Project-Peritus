//! Closed H1 fault and recovery vocabulary.

/// Durable commit boundaries named by production acceptance criterion 7.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CommitBoundary {
    /// Authoritative journal append.
    Journal,
    /// Content-addressed blob finalization and reference publication.
    Blob,
    /// Workspace snapshot publication.
    Snapshot,
    /// Exclusive mutation-lease transition.
    Lease,
    /// Atomic patch candidate commit.
    Patch,
    /// Revision-bound gate evidence commit.
    Gate,
    /// Harness promotion pointer commit.
    Promotion,
}

impl CommitBoundary {
    /// All production commit boundaries in stable order.
    pub const ALL: [Self; 7] = [
        Self::Journal,
        Self::Blob,
        Self::Snapshot,
        Self::Lease,
        Self::Patch,
        Self::Gate,
        Self::Promotion,
    ];

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::Blob => "blob",
            Self::Snapshot => "snapshot",
            Self::Lease => "lease",
            Self::Patch => "patch",
            Self::Gate => "gate",
            Self::Promotion => "promotion",
        }
    }
}

/// Side of a durable commit at which power loss is injected.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CrashTiming {
    /// Intent exists but the authoritative commit is not durable.
    BeforeDurableCommit,
    /// Commit is durable but its caller has not received an acknowledgement.
    AfterDurableCommitBeforeAck,
}

/// State target made corrupt or hash-divergent.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CorruptTarget {
    /// Journal hash chain or sequence.
    Journal,
    /// Referenced content-addressed blob.
    Blob,
    /// Referenced workspace snapshot.
    Snapshot,
    /// Rebuildable projection.
    Projection,
    /// Revision-bound acceptance evidence.
    AcceptanceEvidence,
    /// Immutable harness revision or promotion pointer.
    HarnessPromotion,
}

impl CorruptTarget {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::Blob => "blob",
            Self::Snapshot => "snapshot",
            Self::Projection => "projection",
            Self::AcceptanceEvidence => "acceptance-evidence",
            Self::HarnessPromotion => "harness-promotion",
        }
    }
}

/// Disk/quota boundary exhausted by a scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiskScope {
    /// Journal/database transaction append.
    JournalAppend,
    /// Temporary-to-final blob publication.
    BlobFinalize,
    /// Snapshot publication.
    SnapshotCommit,
}

impl DiskScope {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::JournalAppend => "journal-append",
            Self::BlobFinalize => "blob-finalize",
            Self::SnapshotCommit => "snapshot-commit",
        }
    }
}

/// External or supervised component terminated by a scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DependencyKind {
    /// Model provider request or stream.
    Provider,
    /// Tool process or extension endpoint.
    Tool,
    /// Daemon-owned worker task/process.
    Worker,
}

impl DependencyKind {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Tool => "tool",
            Self::Worker => "worker",
        }
    }
}

/// Every active E0 lifecycle phase in architecture order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DaemonLifecyclePhase {
    /// Writer directive awaits durable publication.
    WriterPending,
    /// Writer child is active.
    WriterActive,
    /// Gate directive awaits durable publication.
    GatesPending,
    /// Gate child is active.
    GatesActive,
    /// Review directive awaits durable publication.
    ReviewPending,
    /// Review child is active.
    ReviewActive,
    /// Fixer directive awaits durable publication.
    FixerPending,
    /// Fixer child is active.
    FixerActive,
    /// Candidate revision is advancing atomically.
    RevisionAdvancing,
    /// Acceptance policy is being evaluated.
    EvaluatingAcceptance,
    /// Durable kernel acceptance truth is pending.
    KernelAcceptancePending,
}

impl DaemonLifecyclePhase {
    /// All E0 active phases in stable architecture order.
    pub const ALL: [Self; 11] = [
        Self::WriterPending,
        Self::WriterActive,
        Self::GatesPending,
        Self::GatesActive,
        Self::ReviewPending,
        Self::ReviewActive,
        Self::FixerPending,
        Self::FixerActive,
        Self::RevisionAdvancing,
        Self::EvaluatingAcceptance,
        Self::KernelAcceptancePending,
    ];

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::WriterPending => "writer-pending",
            Self::WriterActive => "writer-active",
            Self::GatesPending => "gates-pending",
            Self::GatesActive => "gates-active",
            Self::ReviewPending => "review-pending",
            Self::ReviewActive => "review-active",
            Self::FixerPending => "fixer-pending",
            Self::FixerActive => "fixer-active",
            Self::RevisionAdvancing => "revision-advancing",
            Self::EvaluatingAcceptance => "evaluating-acceptance",
            Self::KernelAcceptancePending => "kernel-acceptance-pending",
        }
    }
}

/// Host reboot points requiring startup reconciliation evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RebootPhase {
    /// An external effect is outstanding.
    OutstandingEffect,
    /// Durable commit exists but acknowledgement does not.
    DurableBeforeAck,
    /// Startup is itself reconciling recovered ownership.
    StartupReconciliation,
}

impl RebootPhase {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::OutstandingEffect => "outstanding-effect",
            Self::DurableBeforeAck => "durable-before-ack",
            Self::StartupReconciliation => "startup-reconciliation",
        }
    }
}

/// Exact disruption required by one scenario.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FaultInjection {
    /// Power loss around one durable commit.
    CommitCrash {
        /// Durable component boundary under qualification.
        boundary: CommitBoundary,
        /// Side of the durable commit where power loss occurs.
        timing: CrashTiming,
    },
    /// Corrupt or hash-divergent state.
    Corruption(CorruptTarget),
    /// Disk/quota exhaustion.
    DiskExhaustion(DiskScope),
    /// Provider, tool, or worker death during owned work.
    DependencyDeath(DependencyKind),
    /// The retry ceiling is reached with no successful observation.
    RetryExhaustion(DependencyKind),
    /// Daemon process death in an active lifecycle phase.
    DaemonKill(DaemonLifecyclePhase),
    /// Host reboot requiring durable startup reconciliation.
    HostReboot(RebootPhase),
}

/// Documented authoritative recovery classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RecoveryOutcome {
    /// Non-durable intent was rolled back without authority change.
    RolledBackUncommitted,
    /// A durable commit was replayed without duplicating it.
    ReplayedCommitted,
    /// A disposable projection was rebuilt from the authoritative journal.
    RebuiltProjection,
    /// Corrupt state was quarantined and excluded from authority.
    QuarantinedCorruption,
    /// Mutation stopped while read-only diagnostics remained truthful.
    FailedClosed,
    /// An unreferenced temporary object was discarded.
    DiscardedUnreferenced,
    /// Owned work was resumed, failed, or marked indeterminate explicitly.
    ReconciledOwnedWork,
    /// The governed retry budget terminated the attempt without success.
    RetryBudgetExhausted,
}
