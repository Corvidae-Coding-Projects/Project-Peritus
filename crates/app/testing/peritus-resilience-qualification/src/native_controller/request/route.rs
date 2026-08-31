//! Closed mapping from declared H1 scenarios to production effect routes.

use super::{FaultDocument, ScenarioDocument};

pub(super) const BLOB_BEFORE: &str = "h1.crash.blob.before";
pub(super) const BLOB_AFTER_BEFORE_ACK: &str = "h1.crash.blob.after-before-ack";
pub(super) const BLOB_CORRUPTION: &str = "h1.corruption.blob";
pub(super) const BLOB_FINALIZE_DISK_EXHAUSTION: &str = "h1.disk-full.blob-finalize";
pub(super) const JOURNAL_BEFORE: &str = "h1.crash.journal.before";
pub(super) const JOURNAL_AFTER_BEFORE_ACK: &str = "h1.crash.journal.after-before-ack";
pub(super) const JOURNAL_CORRUPTION: &str = "h1.corruption.journal";
pub(super) const LEASE_BEFORE: &str = "h1.crash.lease.before";
pub(super) const LEASE_AFTER_BEFORE_ACK: &str = "h1.crash.lease.after-before-ack";
pub(super) const GATE_BEFORE: &str = "h1.crash.gate.before";
pub(super) const GATE_AFTER_BEFORE_ACK: &str = "h1.crash.gate.after-before-ack";
pub(super) const PATCH_BEFORE: &str = "h1.crash.patch.before";
pub(super) const PATCH_AFTER_BEFORE_ACK: &str = "h1.crash.patch.after-before-ack";
pub(super) const SNAPSHOT_BEFORE: &str = "h1.crash.snapshot.before";
pub(super) const SNAPSHOT_AFTER_BEFORE_ACK: &str = "h1.crash.snapshot.after-before-ack";
pub(super) const SNAPSHOT_CORRUPTION: &str = "h1.corruption.snapshot";
pub(super) const PROMOTION_BEFORE: &str = "h1.crash.promotion.before";
pub(super) const PROMOTION_AFTER_BEFORE_ACK: &str = "h1.crash.promotion.after-before-ack";
pub(super) const PROJECTION_CORRUPTION: &str = "h1.corruption.projection";
pub(super) const PROVIDER_DEATH: &str = "h1.death.provider";
pub(super) const TOOL_DEATH: &str = "h1.death.tool";
pub(super) const WORKER_DEATH: &str = "h1.death.worker";
pub(super) const PROVIDER_RETRY_EXHAUSTION: &str = "h1.retry-exhaustion.provider";
pub(super) const TOOL_RETRY_EXHAUSTION: &str = "h1.retry-exhaustion.tool";
pub(super) const WORKER_RETRY_EXHAUSTION: &str = "h1.retry-exhaustion.worker";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_controller) enum ScenarioRoute {
    BlobBeforeDurableCommit,
    BlobAfterDurableCommitBeforeAck,
    BlobCorruption,
    BlobFinalizeDiskExhaustion,
    JournalBeforeDurableCommit,
    JournalAfterDurableCommitBeforeAck,
    JournalCorruption,
    LeaseBeforeDurableCommit,
    LeaseAfterDurableCommitBeforeAck,
    GateBeforeDurableCommit,
    GateAfterDurableCommitBeforeAck,
    PatchBeforeDurableCommit,
    PatchAfterDurableCommitBeforeAck,
    SnapshotBeforeDurableCommit,
    SnapshotAfterDurableCommitBeforeAck,
    SnapshotCorruption,
    PromotionBeforeDurableCommit,
    PromotionAfterDurableCommitBeforeAck,
    ProjectionCorruption,
    ProviderDeath,
    ToolDeath,
    WorkerDeath,
    ProviderRetryExhaustion,
    ToolRetryExhaustion,
    WorkerRetryExhaustion,
}

impl ScenarioRoute {
    pub(super) fn from_scenario(scenario: &ScenarioDocument) -> Option<Self> {
        match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
            (
                BLOB_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "blob" && timing == "before-durable-commit" => {
                Some(Self::BlobBeforeDurableCommit)
            }
            (
                BLOB_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "blob" && timing == "after-durable-commit-before-ack" => {
                Some(Self::BlobAfterDurableCommitBeforeAck)
            }
            (
                JOURNAL_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "journal" && timing == "before-durable-commit" => {
                Some(Self::JournalBeforeDurableCommit)
            }
            (
                JOURNAL_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "journal" && timing == "after-durable-commit-before-ack" => {
                Some(Self::JournalAfterDurableCommitBeforeAck)
            }
            (
                LEASE_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "lease" && timing == "before-durable-commit" => {
                Some(Self::LeaseBeforeDurableCommit)
            }
            (
                LEASE_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "lease" && timing == "after-durable-commit-before-ack" => {
                Some(Self::LeaseAfterDurableCommitBeforeAck)
            }
            (
                GATE_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "gate" && timing == "before-durable-commit" => {
                Some(Self::GateBeforeDurableCommit)
            }
            (
                GATE_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "gate" && timing == "after-durable-commit-before-ack" => {
                Some(Self::GateAfterDurableCommitBeforeAck)
            }
            (
                PATCH_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "patch" && timing == "before-durable-commit" => {
                Some(Self::PatchBeforeDurableCommit)
            }
            (
                PATCH_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "patch" && timing == "after-durable-commit-before-ack" => {
                Some(Self::PatchAfterDurableCommitBeforeAck)
            }
            (
                SNAPSHOT_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "snapshot" && timing == "before-durable-commit" => {
                Some(Self::SnapshotBeforeDurableCommit)
            }
            (
                SNAPSHOT_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "snapshot" && timing == "after-durable-commit-before-ack" => {
                Some(Self::SnapshotAfterDurableCommitBeforeAck)
            }
            _ => Self::from_promotion(scenario),
        }
    }

    fn from_promotion(scenario: &ScenarioDocument) -> Option<Self> {
        match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
            (
                PROMOTION_BEFORE,
                "rolled-back-uncommitted",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "promotion" && timing == "before-durable-commit" => {
                Some(Self::PromotionBeforeDurableCommit)
            }
            (
                PROMOTION_AFTER_BEFORE_ACK,
                "replayed-committed",
                FaultDocument::CommitCrash { boundary, timing },
            ) if boundary == "promotion" && timing == "after-durable-commit-before-ack" => {
                Some(Self::PromotionAfterDurableCommitBeforeAck)
            }
            _ => Self::from_corruption(scenario),
        }
    }

    fn from_corruption(scenario: &ScenarioDocument) -> Option<Self> {
        match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
            (BLOB_CORRUPTION, "quarantined-corruption", FaultDocument::Corruption { target })
                if target == "blob" =>
            {
                Some(Self::BlobCorruption)
            }
            (JOURNAL_CORRUPTION, "failed-closed", FaultDocument::Corruption { target })
                if target == "journal" =>
            {
                Some(Self::JournalCorruption)
            }
            (
                SNAPSHOT_CORRUPTION,
                "quarantined-corruption",
                FaultDocument::Corruption { target },
            ) if target == "snapshot" => Some(Self::SnapshotCorruption),
            (PROJECTION_CORRUPTION, "rebuilt-projection", FaultDocument::Corruption { target })
                if target == "projection" =>
            {
                Some(Self::ProjectionCorruption)
            }
            _ => Self::from_disk_exhaustion(scenario),
        }
    }

    fn from_disk_exhaustion(scenario: &ScenarioDocument) -> Option<Self> {
        match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
            (
                BLOB_FINALIZE_DISK_EXHAUSTION,
                "discarded-unreferenced",
                FaultDocument::DiskExhaustion { scope },
            ) if scope == "blob-finalize" => Some(Self::BlobFinalizeDiskExhaustion),
            _ => Self::from_dependency(scenario),
        }
    }

    fn from_dependency(scenario: &ScenarioDocument) -> Option<Self> {
        match (&*scenario.id, &*scenario.expected_recovery, &scenario.fault) {
            (
                PROVIDER_DEATH,
                "reconciled-owned-work",
                FaultDocument::DependencyDeath { dependency },
            ) if dependency == "provider" => Some(Self::ProviderDeath),
            (
                TOOL_DEATH,
                "reconciled-owned-work",
                FaultDocument::DependencyDeath { dependency },
            ) if dependency == "tool" => Some(Self::ToolDeath),
            (
                WORKER_DEATH,
                "reconciled-owned-work",
                FaultDocument::DependencyDeath { dependency },
            ) if dependency == "worker" => Some(Self::WorkerDeath),
            (
                PROVIDER_RETRY_EXHAUSTION,
                "retry-budget-exhausted",
                FaultDocument::RetryExhaustion { dependency },
            ) if dependency == "provider" => Some(Self::ProviderRetryExhaustion),
            (
                TOOL_RETRY_EXHAUSTION,
                "retry-budget-exhausted",
                FaultDocument::RetryExhaustion { dependency },
            ) if dependency == "tool" => Some(Self::ToolRetryExhaustion),
            (
                WORKER_RETRY_EXHAUSTION,
                "retry-budget-exhausted",
                FaultDocument::RetryExhaustion { dependency },
            ) if dependency == "worker" => Some(Self::WorkerRetryExhaustion),
            _ => None,
        }
    }

    /// Returns the recovery outcome required for this route.
    pub(in crate::native_controller) const fn outcome(self) -> &'static str {
        match self {
            Self::BlobBeforeDurableCommit
            | Self::JournalBeforeDurableCommit
            | Self::LeaseBeforeDurableCommit
            | Self::GateBeforeDurableCommit
            | Self::PatchBeforeDurableCommit
            | Self::SnapshotBeforeDurableCommit
            | Self::PromotionBeforeDurableCommit => "rolled-back-uncommitted",
            Self::BlobAfterDurableCommitBeforeAck
            | Self::JournalAfterDurableCommitBeforeAck
            | Self::LeaseAfterDurableCommitBeforeAck
            | Self::GateAfterDurableCommitBeforeAck
            | Self::PatchAfterDurableCommitBeforeAck
            | Self::SnapshotAfterDurableCommitBeforeAck
            | Self::PromotionAfterDurableCommitBeforeAck => "replayed-committed",
            Self::ProjectionCorruption => "rebuilt-projection",
            Self::BlobCorruption | Self::SnapshotCorruption => "quarantined-corruption",
            Self::BlobFinalizeDiskExhaustion => "discarded-unreferenced",
            Self::JournalCorruption => "failed-closed",
            Self::ProviderDeath | Self::ToolDeath | Self::WorkerDeath => "reconciled-owned-work",
            Self::ProviderRetryExhaustion
            | Self::ToolRetryExhaustion
            | Self::WorkerRetryExhaustion => "retry-budget-exhausted",
        }
    }

    /// Returns the directly supportable journal-health observation.
    pub(in crate::native_controller) const fn journal_health(self) -> &'static str {
        match self {
            Self::JournalCorruption => "hash-divergence-detected",
            Self::BlobBeforeDurableCommit
            | Self::BlobAfterDurableCommitBeforeAck
            | Self::JournalBeforeDurableCommit
            | Self::LeaseBeforeDurableCommit
            | Self::LeaseAfterDurableCommitBeforeAck
            | Self::GateBeforeDurableCommit
            | Self::GateAfterDurableCommitBeforeAck
            | Self::PatchBeforeDurableCommit
            | Self::PatchAfterDurableCommitBeforeAck
            | Self::SnapshotBeforeDurableCommit
            | Self::SnapshotAfterDurableCommitBeforeAck
            | Self::PromotionBeforeDurableCommit
            | Self::PromotionAfterDurableCommitBeforeAck
            | Self::ProjectionCorruption
            | Self::BlobCorruption
            | Self::BlobFinalizeDiskExhaustion
            | Self::SnapshotCorruption
            | Self::ProviderDeath
            | Self::ToolDeath
            | Self::WorkerDeath
            | Self::ProviderRetryExhaustion
            | Self::ToolRetryExhaustion
            | Self::WorkerRetryExhaustion => "verified",
            Self::JournalAfterDurableCommitBeforeAck => "recovered-and-verified",
        }
    }

    pub(in crate::native_controller) const fn projection_health(self) -> &'static str {
        match self {
            Self::ProjectionCorruption => "rebuilt-and-verified",
            Self::JournalCorruption => "unavailable",
            _ => "verified",
        }
    }

    pub(in crate::native_controller) const fn artifact_health(self) -> &'static str {
        match self {
            Self::BlobCorruption | Self::SnapshotCorruption => "divergence-detected",
            _ => "verified",
        }
    }

    pub(in crate::native_controller) const fn corruption_target(self) -> Option<&'static str> {
        match self {
            Self::ProjectionCorruption => Some("projection"),
            Self::BlobCorruption => Some("blob"),
            Self::SnapshotCorruption => Some("snapshot"),
            Self::JournalCorruption => Some("journal"),
            _ => None,
        }
    }

    pub(in crate::native_controller) const fn mutation_admitted(self) -> bool {
        !matches!(
            self,
            Self::ProjectionCorruption
                | Self::JournalCorruption
                | Self::BlobCorruption
                | Self::SnapshotCorruption
        )
    }

    pub(in crate::native_controller) const fn dependency(self) -> Option<&'static str> {
        match self {
            Self::ProviderDeath | Self::ProviderRetryExhaustion => Some("provider"),
            Self::ToolDeath | Self::ToolRetryExhaustion => Some("tool"),
            Self::WorkerDeath | Self::WorkerRetryExhaustion => Some("worker"),
            _ => None,
        }
    }

    pub(in crate::native_controller) const fn dependency_fault(self) -> Option<&'static str> {
        match self {
            Self::ProviderDeath | Self::ToolDeath | Self::WorkerDeath => Some("death"),
            Self::ProviderRetryExhaustion
            | Self::ToolRetryExhaustion
            | Self::WorkerRetryExhaustion => Some("retry-exhaustion"),
            _ => None,
        }
    }

    pub(in crate::native_controller) const fn is_retry_exhaustion(self) -> bool {
        matches!(
            self,
            Self::ProviderRetryExhaustion | Self::ToolRetryExhaustion | Self::WorkerRetryExhaustion
        )
    }
}
