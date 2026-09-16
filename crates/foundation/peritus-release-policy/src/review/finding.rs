//! Release finding observations.

use crate::{ConstructionError, EvidenceBinding, FindingId, PrincipalId};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Stable release finding severity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    /// Informational concern.
    Informational,
    /// Low-impact concern.
    Low,
    /// Material but non-release-blocking concern.
    Medium,
    /// High-impact concern.
    High,
    /// Critical concern.
    Critical,
}

/// Current explicit finding disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FindingDisposition {
    /// Finding remains open.
    Open,
    /// Finding was resolved with retained evidence.
    Resolved,
    /// Finding requests an independent waiver.
    WaiverRequested,
    /// Finding was ignored; this always blocks release.
    Ignored,
    /// Finding was quarantined; this always blocks release.
    Quarantined,
}

/// One current finding state bound to the exact candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FindingObservation {
    id: FindingId,
    binding: EvidenceBinding,
    reporter: PrincipalId,
    severity: FindingSeverity,
    release_blocking: bool,
    disposition: FindingDisposition,
    finding_digest: Sha256Digest,
}

impl FindingObservation {
    /// Creates one signed finding-state observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder finding digest.
    pub fn new(
        id: FindingId,
        binding: EvidenceBinding,
        reporter: PrincipalId,
        severity: FindingSeverity,
        release_blocking: bool,
        disposition: FindingDisposition,
        finding_digest: Sha256Digest,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == crate::validation::spec_digest_nonzero(finding_digest),
            match result {
                Ok(value) => value.spec_id() == id
                    && value.spec_binding() == binding
                    && value.spec_reporter() == reporter
                    && value.spec_severity() == severity
                    && value.spec_release_blocking() == release_blocking
                    && value.spec_disposition() == disposition
                    && value.spec_finding_digest() == finding_digest,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(finding_digest)?;
        Ok(Self {
            id,
            binding,
            reporter,
            severity,
            release_blocking,
            disposition,
            finding_digest,
        })
    }

    /// Returns the stable finding identity.
    #[must_use]
    pub const fn id(&self) -> (id: FindingId) ensures id == self.spec_id() { self.id }

    /// Returns the exact evidence binding.
    #[must_use]
    pub const fn binding(&self) -> (binding: EvidenceBinding)
        ensures binding == self.spec_binding()
    {
        self.binding
    }

    /// Returns the finding reporter.
    #[must_use]
    pub const fn reporter(&self) -> (reporter: PrincipalId)
        ensures reporter == self.spec_reporter()
    {
        self.reporter
    }

    /// Returns the severity.
    #[must_use]
    pub const fn severity(&self) -> (severity: FindingSeverity)
        ensures severity == self.spec_severity()
    {
        self.severity
    }

    /// Returns whether policy marked the finding release-blocking.
    #[must_use]
    pub const fn release_blocking(&self) -> (blocking: bool)
        ensures blocking == self.spec_release_blocking()
    {
        self.release_blocking
    }

    /// Returns the current disposition.
    #[must_use]
    pub const fn disposition(&self) -> (disposition: FindingDisposition)
        ensures disposition == self.spec_disposition()
    {
        self.disposition
    }

    /// Returns the signed finding-state digest.
    #[must_use]
    pub const fn finding_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_finding_digest()
    {
        self.finding_digest
    }

    /// Logical view of the stable finding identity.
    pub closed spec fn spec_id(&self) -> FindingId { self.id }

    /// Logical view of the exact evidence binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Logical view of the finding reporter.
    pub closed spec fn spec_reporter(&self) -> PrincipalId { self.reporter }

    /// Logical view of the finding severity.
    pub closed spec fn spec_severity(&self) -> FindingSeverity { self.severity }

    /// Logical view of whether the finding is release blocking.
    pub closed spec fn spec_release_blocking(&self) -> bool { self.release_blocking }

    /// Logical view of the current disposition.
    pub closed spec fn spec_disposition(&self) -> FindingDisposition { self.disposition }

    /// Logical view of the signed finding-state digest.
    pub closed spec fn spec_finding_digest(&self) -> Sha256Digest { self.finding_digest }

    /// Logical equality of every supplied finding field.
    pub open spec fn spec_matches(&self, other: FindingObservation) -> bool {
        crate::identity::finding_ids_match(self.spec_id(), other.spec_id())
            && crate::evidence::bindings_match(self.spec_binding(), other.spec_binding())
            && crate::identity::principal_ids_match(
                self.spec_reporter(),
                other.spec_reporter(),
            )
            && self.spec_severity() == other.spec_severity()
            && self.spec_release_blocking() == other.spec_release_blocking()
            && self.spec_disposition() == other.spec_disposition()
            && crate::candidate::digest_matches(
                self.spec_finding_digest(),
                other.spec_finding_digest(),
            )
    }
}

} // verus!
