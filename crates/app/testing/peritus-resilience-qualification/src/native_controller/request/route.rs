//! Closed mapping from declared H1 scenarios to production effect routes.

use super::{FaultDocument, ScenarioDocument};

pub(super) const BLOB_BEFORE: &str = "h1.crash.blob.before";
pub(super) const BLOB_AFTER_BEFORE_ACK: &str = "h1.crash.blob.after-before-ack";
pub(super) const JOURNAL_BEFORE: &str = "h1.crash.journal.before";
pub(super) const JOURNAL_AFTER_BEFORE_ACK: &str = "h1.crash.journal.after-before-ack";
pub(super) const LEASE_BEFORE: &str = "h1.crash.lease.before";
pub(super) const LEASE_AFTER_BEFORE_ACK: &str = "h1.crash.lease.after-before-ack";
pub(super) const GATE_BEFORE: &str = "h1.crash.gate.before";
pub(super) const GATE_AFTER_BEFORE_ACK: &str = "h1.crash.gate.after-before-ack";
pub(super) const PATCH_BEFORE: &str = "h1.crash.patch.before";
pub(super) const PATCH_AFTER_BEFORE_ACK: &str = "h1.crash.patch.after-before-ack";
pub(super) const SNAPSHOT_BEFORE: &str = "h1.crash.snapshot.before";
pub(super) const SNAPSHOT_AFTER_BEFORE_ACK: &str = "h1.crash.snapshot.after-before-ack";
pub(super) const PROMOTION_BEFORE: &str = "h1.crash.promotion.before";
pub(super) const PROMOTION_AFTER_BEFORE_ACK: &str = "h1.crash.promotion.after-before-ack";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::native_controller) enum CommitRoute {
    BlobBeforeDurableCommit,
    BlobAfterDurableCommitBeforeAck,
    JournalBeforeDurableCommit,
    JournalAfterDurableCommitBeforeAck,
    LeaseBeforeDurableCommit,
    LeaseAfterDurableCommitBeforeAck,
    GateBeforeDurableCommit,
    GateAfterDurableCommitBeforeAck,
    PatchBeforeDurableCommit,
    PatchAfterDurableCommitBeforeAck,
    SnapshotBeforeDurableCommit,
    SnapshotAfterDurableCommitBeforeAck,
    PromotionBeforeDurableCommit,
    PromotionAfterDurableCommitBeforeAck,
}

impl CommitRoute {
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
            _ => None,
        }
    }

    /// Returns the recovery outcome required for this side of the commit.
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
        }
    }

    /// Returns the directly supportable journal-health observation.
    pub(in crate::native_controller) const fn journal_health(self) -> &'static str {
        match self {
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
            | Self::PromotionAfterDurableCommitBeforeAck => "verified",
            Self::JournalAfterDurableCommitBeforeAck => "recovered-and-verified",
        }
    }
}
