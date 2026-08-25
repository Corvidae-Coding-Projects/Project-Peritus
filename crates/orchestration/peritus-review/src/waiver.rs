//! Consumption-only wrapper for external B2 waiver observations.

use peritus_quality_policy::WaiverObservation;
use peritus_types::{FindingId, RevisionTuple, Sha256Digest};

/// An inert external waiver observation matched to one prior D2 request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObservedWaiver {
    observation: WaiverObservation,
    request_digest: Sha256Digest,
}

impl ObservedWaiver {
    /// Wraps an existing external observation without constructing or authorizing it.
    #[must_use]
    pub const fn from_external(
        observation: WaiverObservation,
        request_digest: Sha256Digest,
    ) -> Self {
        Self { observation, request_digest }
    }

    pub(super) const fn from_wire(
        observation: WaiverObservation,
        request_digest: Sha256Digest,
    ) -> Self {
        Self { observation, request_digest }
    }

    /// Returns the exact external B2 observation.
    #[must_use]
    pub const fn observation(&self) -> WaiverObservation {
        self.observation
    }
    /// Returns the finding authorized by the observation.
    #[must_use]
    pub const fn finding_id(&self) -> FindingId {
        self.observation.finding_id()
    }
    /// Returns the exact authorized revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.observation.revision()
    }
    /// Returns the previously recorded request digest this observation answers.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }
}
