//! Stable scalar tags for the canonical evidence encoding.

use crate::{
    ArtifactHealth, CommitBoundary, CorruptTarget, CrashTiming, DaemonLifecyclePhase,
    DependencyKind, DiskScope, EvidenceKind, FailurePhase, JournalHealth, MilestoneKind,
    ProjectionHealth, RebootPhase, RecoveryOutcome, ResourceKind, TerminalState,
};

pub const fn failure_phase(value: FailurePhase) -> u8 {
    match value {
        FailurePhase::Definition => 1,
        FailurePhase::Setup => 2,
        FailurePhase::Preparation => 3,
        FailurePhase::Injection => 4,
        FailurePhase::Recovery => 5,
        FailurePhase::Cleanup => 6,
    }
}

pub const fn recovery_outcome(value: RecoveryOutcome) -> u8 {
    match value {
        RecoveryOutcome::RolledBackUncommitted => 1,
        RecoveryOutcome::ReplayedCommitted => 2,
        RecoveryOutcome::RebuiltProjection => 3,
        RecoveryOutcome::QuarantinedCorruption => 4,
        RecoveryOutcome::FailedClosed => 5,
        RecoveryOutcome::DiscardedUnreferenced => 6,
        RecoveryOutcome::ReconciledOwnedWork => 7,
        RecoveryOutcome::RetryBudgetExhausted => 8,
    }
}

pub const fn commit_boundary(value: CommitBoundary) -> u8 {
    match value {
        CommitBoundary::Journal => 1,
        CommitBoundary::Blob => 2,
        CommitBoundary::Snapshot => 3,
        CommitBoundary::Lease => 4,
        CommitBoundary::Patch => 5,
        CommitBoundary::Gate => 6,
        CommitBoundary::Promotion => 7,
    }
}

pub const fn crash_timing(value: CrashTiming) -> u8 {
    match value {
        CrashTiming::BeforeDurableCommit => 1,
        CrashTiming::AfterDurableCommitBeforeAck => 2,
    }
}

pub const fn corrupt_target(value: CorruptTarget) -> u8 {
    match value {
        CorruptTarget::Journal => 1,
        CorruptTarget::Blob => 2,
        CorruptTarget::Snapshot => 3,
        CorruptTarget::Projection => 4,
        CorruptTarget::AcceptanceEvidence => 5,
        CorruptTarget::HarnessPromotion => 6,
    }
}

pub const fn disk_scope(value: DiskScope) -> u8 {
    match value {
        DiskScope::JournalAppend => 1,
        DiskScope::BlobFinalize => 2,
        DiskScope::SnapshotCommit => 3,
    }
}

pub const fn dependency(value: DependencyKind) -> u8 {
    match value {
        DependencyKind::Provider => 1,
        DependencyKind::Tool => 2,
        DependencyKind::Worker => 3,
    }
}

pub const fn daemon_phase(value: DaemonLifecyclePhase) -> u8 {
    match value {
        DaemonLifecyclePhase::WriterPending => 1,
        DaemonLifecyclePhase::WriterActive => 2,
        DaemonLifecyclePhase::GatesPending => 3,
        DaemonLifecyclePhase::GatesActive => 4,
        DaemonLifecyclePhase::ReviewPending => 5,
        DaemonLifecyclePhase::ReviewActive => 6,
        DaemonLifecyclePhase::FixerPending => 7,
        DaemonLifecyclePhase::FixerActive => 8,
        DaemonLifecyclePhase::RevisionAdvancing => 9,
        DaemonLifecyclePhase::EvaluatingAcceptance => 10,
        DaemonLifecyclePhase::KernelAcceptancePending => 11,
    }
}

pub const fn reboot_phase(value: RebootPhase) -> u8 {
    match value {
        RebootPhase::OutstandingEffect => 1,
        RebootPhase::DurableBeforeAck => 2,
        RebootPhase::StartupReconciliation => 3,
    }
}

pub const fn terminal(value: TerminalState) -> u8 {
    match value {
        TerminalState::Active => 1,
        TerminalState::Paused => 2,
        TerminalState::Blocked => 3,
        TerminalState::Failed => 4,
        TerminalState::Cancelled => 5,
        TerminalState::Exhausted => 6,
        TerminalState::Accepted => 7,
    }
}

pub const fn journal(value: JournalHealth) -> u8 {
    match value {
        JournalHealth::Verified => 1,
        JournalHealth::RecoveredAndVerified => 2,
        JournalHealth::HashDivergenceDetected => 3,
        JournalHealth::Unavailable => 4,
    }
}

pub const fn artifacts(value: ArtifactHealth) -> u8 {
    match value {
        ArtifactHealth::Verified => 1,
        ArtifactHealth::DivergenceDetected => 2,
        ArtifactHealth::Unavailable => 3,
    }
}

pub const fn projection(value: ProjectionHealth) -> u8 {
    match value {
        ProjectionHealth::Verified => 1,
        ProjectionHealth::RebuiltAndVerified => 2,
        ProjectionHealth::Divergent => 3,
        ProjectionHealth::Unavailable => 4,
    }
}

pub const fn evidence_kind(value: EvidenceKind) -> u8 {
    match value {
        EvidenceKind::FaultInjection => 1,
        EvidenceKind::Journal => 2,
        EvidenceKind::Recovery => 3,
        EvidenceKind::Ownership => 4,
        EvidenceKind::Resource => 5,
        EvidenceKind::FinalState => 6,
    }
}

pub const fn milestone_kind(value: MilestoneKind) -> u8 {
    match value {
        MilestoneKind::Prepared => 1,
        MilestoneKind::FaultArmed => 2,
        MilestoneKind::FaultObserved => 3,
        MilestoneKind::RecoveryStarted => 4,
        MilestoneKind::Reconciled => 5,
        MilestoneKind::Inspected => 6,
    }
}

pub const fn resource_kind(value: ResourceKind) -> u8 {
    match value {
        ResourceKind::Events => 1,
        ResourceKind::EvidenceBytes => 2,
        ResourceKind::OwnedProcesses => 3,
        ResourceKind::CleanupSteps => 4,
        ResourceKind::LogicalTicks => 5,
        ResourceKind::ReconciliationSteps => 6,
        ResourceKind::Milestones => 7,
    }
}
