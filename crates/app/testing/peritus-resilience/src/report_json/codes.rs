//! Stable text spellings for the H1 JSON projection.

use serde_json::{Value, json};

use crate::{
    ArtifactHealth, CatalogProfile, CommitBoundary, ContractViolation, CorruptTarget, CrashTiming,
    DaemonLifecyclePhase, DependencyKind, DiskScope, EvidenceKind, FailurePhase, FaultInjection,
    JournalHealth, MilestoneKind, NotReadyReason, ProjectionHealth, QualificationVerdict,
    RebootPhase, RecoveryOutcome, SubjectErrorCode, TerminalState,
};

pub(super) const fn catalog_profile(value: CatalogProfile) -> &'static str {
    match value {
        CatalogProfile::H1Production => "h1-production",
        CatalogProfile::Custom => "custom",
    }
}

pub(super) const fn verdict(value: QualificationVerdict) -> &'static str {
    match value {
        QualificationVerdict::Ready => "ready",
        QualificationVerdict::NotReadyForProduction(NotReadyReason::CustomCatalog) => {
            "not-ready-custom-catalog"
        }
        QualificationVerdict::NotReadyForProduction(NotReadyReason::SuiteFailure) => {
            "not-ready-suite-failure"
        }
        QualificationVerdict::NotReadyForProduction(NotReadyReason::ScenarioFailure) => {
            "not-ready-scenario-failure"
        }
    }
}

pub(super) fn fault(value: FaultInjection) -> Value {
    match value {
        FaultInjection::CommitCrash { boundary, timing } => json!({
            "kind": "commit-crash",
            "boundary": commit_boundary(boundary),
            "timing": crash_timing(timing),
        }),
        FaultInjection::Corruption(target) => {
            json!({"kind": "corruption", "target": corrupt_target(target)})
        }
        FaultInjection::DiskExhaustion(scope) => {
            json!({"kind": "disk-exhaustion", "scope": disk_scope(scope)})
        }
        FaultInjection::DependencyDeath(dependency) => json!({
            "kind": "dependency-death",
            "dependency": dependency_kind(dependency),
        }),
        FaultInjection::RetryExhaustion(dependency) => json!({
            "kind": "retry-exhaustion",
            "dependency": dependency_kind(dependency),
        }),
        FaultInjection::DaemonKill(phase) => {
            json!({"kind": "daemon-kill", "phase": daemon_phase(phase)})
        }
        FaultInjection::HostReboot(phase) => {
            json!({"kind": "host-reboot", "phase": reboot_phase(phase)})
        }
    }
}

const fn commit_boundary(value: CommitBoundary) -> &'static str {
    match value {
        CommitBoundary::Journal => "journal",
        CommitBoundary::Blob => "blob",
        CommitBoundary::Snapshot => "snapshot",
        CommitBoundary::Lease => "lease",
        CommitBoundary::Patch => "patch",
        CommitBoundary::Gate => "gate",
        CommitBoundary::Promotion => "promotion",
    }
}

const fn crash_timing(value: CrashTiming) -> &'static str {
    match value {
        CrashTiming::BeforeDurableCommit => "before-durable-commit",
        CrashTiming::AfterDurableCommitBeforeAck => "after-durable-commit-before-ack",
    }
}

pub(super) const fn corrupt_target(value: CorruptTarget) -> &'static str {
    match value {
        CorruptTarget::Journal => "journal",
        CorruptTarget::Blob => "blob",
        CorruptTarget::Snapshot => "snapshot",
        CorruptTarget::Projection => "projection",
        CorruptTarget::AcceptanceEvidence => "acceptance-evidence",
        CorruptTarget::HarnessPromotion => "harness-promotion",
    }
}

const fn disk_scope(value: DiskScope) -> &'static str {
    match value {
        DiskScope::JournalAppend => "journal-append",
        DiskScope::BlobFinalize => "blob-finalize",
        DiskScope::SnapshotCommit => "snapshot-commit",
    }
}

const fn dependency_kind(value: DependencyKind) -> &'static str {
    match value {
        DependencyKind::Provider => "provider",
        DependencyKind::Tool => "tool",
        DependencyKind::Worker => "worker",
    }
}

const fn daemon_phase(value: DaemonLifecyclePhase) -> &'static str {
    match value {
        DaemonLifecyclePhase::WriterPending => "writer-pending",
        DaemonLifecyclePhase::WriterActive => "writer-active",
        DaemonLifecyclePhase::GatesPending => "gates-pending",
        DaemonLifecyclePhase::GatesActive => "gates-active",
        DaemonLifecyclePhase::ReviewPending => "review-pending",
        DaemonLifecyclePhase::ReviewActive => "review-active",
        DaemonLifecyclePhase::FixerPending => "fixer-pending",
        DaemonLifecyclePhase::FixerActive => "fixer-active",
        DaemonLifecyclePhase::RevisionAdvancing => "revision-advancing",
        DaemonLifecyclePhase::EvaluatingAcceptance => "evaluating-acceptance",
        DaemonLifecyclePhase::KernelAcceptancePending => "kernel-acceptance-pending",
    }
}

const fn reboot_phase(value: RebootPhase) -> &'static str {
    match value {
        RebootPhase::OutstandingEffect => "outstanding-effect",
        RebootPhase::DurableBeforeAck => "durable-before-ack",
        RebootPhase::StartupReconciliation => "startup-reconciliation",
    }
}

pub(super) const fn recovery(value: RecoveryOutcome) -> &'static str {
    match value {
        RecoveryOutcome::RolledBackUncommitted => "rolled-back-uncommitted",
        RecoveryOutcome::ReplayedCommitted => "replayed-committed",
        RecoveryOutcome::RebuiltProjection => "rebuilt-projection",
        RecoveryOutcome::QuarantinedCorruption => "quarantined-corruption",
        RecoveryOutcome::FailedClosed => "failed-closed",
        RecoveryOutcome::DiscardedUnreferenced => "discarded-unreferenced",
        RecoveryOutcome::ReconciledOwnedWork => "reconciled-owned-work",
        RecoveryOutcome::RetryBudgetExhausted => "retry-budget-exhausted",
    }
}

pub(super) const fn terminal(value: TerminalState) -> &'static str {
    match value {
        TerminalState::Active => "active",
        TerminalState::Paused => "paused",
        TerminalState::Blocked => "blocked",
        TerminalState::Failed => "failed",
        TerminalState::Cancelled => "cancelled",
        TerminalState::Exhausted => "exhausted",
        TerminalState::Accepted => "accepted",
    }
}

pub(super) const fn journal(value: JournalHealth) -> &'static str {
    match value {
        JournalHealth::Verified => "verified",
        JournalHealth::RecoveredAndVerified => "recovered-and-verified",
        JournalHealth::HashDivergenceDetected => "hash-divergence-detected",
        JournalHealth::Unavailable => "unavailable",
    }
}

pub(super) const fn artifacts(value: ArtifactHealth) -> &'static str {
    match value {
        ArtifactHealth::Verified => "verified",
        ArtifactHealth::DivergenceDetected => "divergence-detected",
        ArtifactHealth::Unavailable => "unavailable",
    }
}

pub(super) const fn projection(value: ProjectionHealth) -> &'static str {
    match value {
        ProjectionHealth::Verified => "verified",
        ProjectionHealth::RebuiltAndVerified => "rebuilt-and-verified",
        ProjectionHealth::Divergent => "divergent",
        ProjectionHealth::Unavailable => "unavailable",
    }
}

pub(super) const fn evidence_kind(value: EvidenceKind) -> &'static str {
    match value {
        EvidenceKind::FaultInjection => "fault-injection",
        EvidenceKind::Journal => "journal",
        EvidenceKind::Recovery => "recovery",
        EvidenceKind::Ownership => "ownership",
        EvidenceKind::Resource => "resource",
        EvidenceKind::FinalState => "final-state",
    }
}

pub(super) const fn milestone(value: MilestoneKind) -> &'static str {
    match value {
        MilestoneKind::Prepared => "prepared",
        MilestoneKind::FaultArmed => "fault-armed",
        MilestoneKind::FaultObserved => "fault-observed",
        MilestoneKind::RecoveryStarted => "recovery-started",
        MilestoneKind::Reconciled => "reconciled",
        MilestoneKind::Inspected => "inspected",
    }
}

pub(super) const fn failure_phase(value: FailurePhase) -> &'static str {
    match value {
        FailurePhase::Definition => "definition",
        FailurePhase::Setup => "setup",
        FailurePhase::Preparation => "preparation",
        FailurePhase::Injection => "injection",
        FailurePhase::Recovery => "recovery",
        FailurePhase::Cleanup => "cleanup",
    }
}

pub(super) const fn subject_error(value: SubjectErrorCode) -> &'static str {
    match value {
        SubjectErrorCode::Setup => "setup",
        SubjectErrorCode::FaultControl => "fault-control",
        SubjectErrorCode::Persistence => "persistence",
        SubjectErrorCode::Supervision => "supervision",
        SubjectErrorCode::Recovery => "recovery",
        SubjectErrorCode::Observation => "observation",
        SubjectErrorCode::Cleanup => "cleanup",
        SubjectErrorCode::Unsupported => "unsupported",
    }
}

pub(super) const fn contract_violation(value: &ContractViolation) -> &'static str {
    match value {
        ContractViolation::ScenarioIdentityMismatch { .. } => "scenario-identity-mismatch",
        ContractViolation::FaultIdentityMismatch { .. } => "fault-identity-mismatch",
        ContractViolation::BaselineAlreadyAccepted => "baseline-already-accepted",
        ContractViolation::FaultNotReached => "fault-not-reached",
        ContractViolation::UnexpectedRecovery { .. } => "unexpected-recovery",
        ContractViolation::FalseSuccess => "false-success",
        ContractViolation::ContradictoryAcceptanceEvidence => "contradictory-acceptance-evidence",
        ContractViolation::CrashJournalDivergence => "crash-journal-divergence",
        ContractViolation::CorruptionNotDetected { .. } => "corruption-not-detected",
        ContractViolation::UnexpectedCorruption { .. } => "unexpected-corruption",
        ContractViolation::MutationAdmittedWithCorruption => "mutation-admitted-with-corruption",
        ContractViolation::ProjectionNotRebuilt => "projection-not-rebuilt",
        ContractViolation::ReferencedObjectUnverified => "referenced-object-unverified",
        ContractViolation::TemporaryObjectLeak { .. } => "temporary-object-leak",
        ContractViolation::OwnershipScanMissing => "ownership-scan-missing",
        ContractViolation::OwnershipAccountingInvalid => "ownership-accounting-invalid",
        ContractViolation::UnaccountedWork { .. } => "unaccounted-work",
        ContractViolation::OrphanedWork { .. } => "orphaned-work",
        ContractViolation::NoOwnedWorkExercised => "no-owned-work-exercised",
        ContractViolation::RetryLimitExceeded { .. } => "retry-limit-exceeded",
        ContractViolation::RetryExhaustionNotReached { .. } => "retry-exhaustion-not-reached",
        ContractViolation::ResourceLimitExceeded { .. } => "resource-limit-exceeded",
        ContractViolation::MissingEvidence(_) => "missing-evidence",
        ContractViolation::DuplicateEvidence => "duplicate-evidence",
        ContractViolation::NonCanonicalMilestones => "noncanonical-milestones",
        ContractViolation::CleanupIncomplete => "cleanup-incomplete",
    }
}
