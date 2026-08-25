//! Exact-revision gate evidence publication contracts.

mod publication;
mod receipt;

use peritus_spec::EvidenceRequirementId;
use peritus_types::{EventId, EvidenceId, Sha256Digest};

use crate::GateError;

pub use publication::EvidencePublication;
pub use receipt::GateEvidenceReceipt;

/// Maximum required evidence records published by one gate.
pub const MAX_PUBLISHED_GATE_EVIDENCE: usize = 1_024;

/// One admitted C0 evidence record mapped to its B2 declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PublishedGateEvidence {
    requirement_id: EvidenceRequirementId,
    evidence_id: EvidenceId,
    record_digest: Sha256Digest,
    journal_position: u64,
    producing_event: EventId,
}

impl PublishedGateEvidence {
    pub(crate) const fn from_parts(
        requirement_id: EvidenceRequirementId,
        evidence_id: EvidenceId,
        record_digest: Sha256Digest,
        journal_position: u64,
        producing_event: EventId,
    ) -> Self {
        Self { requirement_id, evidence_id, record_digest, journal_position, producing_event }
    }

    /// Returns the exact B2 evidence declaration.
    #[must_use]
    pub const fn requirement_id(self) -> EvidenceRequirementId {
        self.requirement_id
    }

    /// Returns the admitted evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }

    /// Returns the digest of the complete admitted record.
    #[must_use]
    pub const fn record_digest(self) -> Sha256Digest {
        self.record_digest
    }

    /// Returns the exact producing journal position.
    #[must_use]
    pub const fn journal_position(self) -> u64 {
        self.journal_position
    }

    /// Returns the exact producing event identity.
    #[must_use]
    pub const fn producing_event(self) -> EventId {
        self.producing_event
    }
}

/// Effect port for normalized artifact publication and C0 evidence admission.
pub trait GateEvidencePublisher {
    /// Publishes the canonical manifest or resolves the exact idempotent request.
    ///
    /// Implementations create the receipt through [`EvidencePublication::receipt_from_records`]
    /// after publishing [`EvidencePublication::manifest_bytes`] at its advertised digest. Every
    /// required declaration must use a distinct admitted evidence record.
    ///
    /// # Errors
    /// Returns an artifact/evidence/recovery error without granting a gate pass.
    fn publish(
        &mut self,
        publication: &EvidencePublication,
    ) -> Result<GateEvidenceReceipt, GateError>;
}
