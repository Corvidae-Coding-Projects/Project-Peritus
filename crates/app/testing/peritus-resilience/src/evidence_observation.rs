//! Content-addressed evidence anchors and canonical lifecycle chronology.

use crate::{EvidenceDigest, EvidenceId, QualificationText};

pub const HARD_MAX_EVIDENCE_ANCHORS: usize = 32;

/// Required external evidence class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceKind {
    /// Fault-control record proving the exact boundary was reached.
    FaultInjection,
    /// Journal integrity/replay record.
    Journal,
    /// Recovery/reconciliation decision record.
    Recovery,
    /// Owned-work/orphan scan record.
    Ownership,
    /// Retry and resource accounting record.
    Resource,
    /// Final authoritative-state record.
    FinalState,
}

impl EvidenceKind {
    /// Every evidence class required for each H1 scenario.
    pub const REQUIRED: [Self; 6] = [
        Self::FaultInjection,
        Self::Journal,
        Self::Recovery,
        Self::Ownership,
        Self::Resource,
        Self::FinalState,
    ];
}

/// Content-addressed reference to evidence retained by the integrated subject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceAnchor {
    kind: EvidenceKind,
    id: EvidenceId,
    digest: EvidenceDigest,
}

impl EvidenceAnchor {
    /// Creates an evidence reference.
    #[must_use]
    pub const fn new(kind: EvidenceKind, id: EvidenceId, digest: EvidenceDigest) -> Self {
        Self { kind, id, digest }
    }
    /// Returns the evidence class.
    #[must_use]
    pub const fn kind(&self) -> EvidenceKind {
        self.kind
    }
    /// Returns the stable external record identifier.
    #[must_use]
    pub const fn id(&self) -> &EvidenceId {
        &self.id
    }
    /// Returns the exact content digest.
    #[must_use]
    pub const fn digest(&self) -> EvidenceDigest {
        self.digest
    }
}

/// Canonical lifecycle milestone class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneKind {
    /// Isolated active baseline is established.
    Prepared,
    /// Exact deterministic fault control is armed.
    FaultArmed,
    /// Requested fault boundary was reached.
    FaultObserved,
    /// Restart/recovery began.
    RecoveryStarted,
    /// Authoritative and external work reconciliation completed.
    Reconciled,
    /// Final direct inspection completed.
    Inspected,
}

/// One bounded, explicitly sequenced evidence milestone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    sequence: u16,
    kind: MilestoneKind,
    detail: QualificationText,
}

impl Milestone {
    /// Creates a milestone.
    #[must_use]
    pub const fn new(sequence: u16, kind: MilestoneKind, detail: QualificationText) -> Self {
        Self { sequence, kind, detail }
    }
    /// Returns the explicit sequence number.
    #[must_use]
    pub const fn sequence(&self) -> u16 {
        self.sequence
    }
    /// Returns the lifecycle class.
    #[must_use]
    pub const fn kind(&self) -> MilestoneKind {
        self.kind
    }
    /// Returns bounded redacted detail.
    #[must_use]
    pub const fn detail(&self) -> &QualificationText {
        &self.detail
    }
}
