//! Typed findings and exact-revision dispositions.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_spec::FindingSeverity;
use peritus_types::{FindingId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Current disposition of a review finding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FindingDisposition {
    /// The finding remains unresolved.
    Open,
    /// The finding was resolved with evidence against the stated exact revision.
    Resolved {
        /// Revision on which the resolution was checked.
        revision: RevisionTuple,
        /// Evidence supporting the resolution.
        evidence_digest: Sha256Digest,
    },
    /// A human-authorized waiver is requested and must be supplied separately.
    WaiverRequested,
}

/// One normalized finding reported by a reviewer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FindingObservation {
    finding_id: FindingId,
    severity: FindingSeverity,
    disposition: FindingDisposition,
    finding_digest: Sha256Digest,
}

impl FindingObservation {
    /// Specification view of the supplied finding identity.
    pub closed spec fn spec_finding_id(&self) -> FindingId { self.finding_id }

    /// Specification view of the supplied finding severity.
    pub closed spec fn spec_severity(&self) -> FindingSeverity { self.severity }

    /// Specification view of the complete supplied disposition.
    pub closed spec fn spec_disposition(&self) -> FindingDisposition { self.disposition }

    /// Specification view of the supplied finding digest.
    pub closed spec fn spec_finding_digest(&self) -> Sha256Digest { self.finding_digest }

    /// Creates a normalized finding observation.
    #[must_use]
    pub const fn new(
        finding_id: FindingId,
        severity: FindingSeverity,
        disposition: FindingDisposition,
        finding_digest: Sha256Digest,
    ) -> (finding: Self)
        ensures
            finding.spec_finding_id() == finding_id,
            finding.spec_severity() == severity,
            finding.spec_disposition() == disposition,
            finding.spec_finding_digest() == finding_digest,
    {
        Self { finding_id, severity, disposition, finding_digest }
    }

    /// Returns the stable finding identity.
    #[must_use]
    pub const fn finding_id(&self) -> (id: FindingId)
        ensures id == self.spec_finding_id(),
    { self.finding_id }

    /// Returns the normalized severity.
    #[must_use]
    pub const fn severity(&self) -> (severity: FindingSeverity)
        ensures severity == self.spec_severity(),
    { self.severity }

    /// Returns the current disposition.
    #[must_use]
    pub const fn disposition(&self) -> (disposition: FindingDisposition)
        ensures disposition == self.spec_disposition(),
    { self.disposition }

    /// Returns the digest of the normalized finding record.
    #[must_use]
    pub const fn finding_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_finding_digest(),
    { self.finding_digest }
}

} // verus!
