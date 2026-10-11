//! Release finding observations.

use crate::{ConstructionError, EvidenceBinding, FindingId, PrincipalId};
use peritus_types::Sha256Digest;

#[doc = " Stable release finding severity."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    #[doc = " Informational concern."]
    Informational,
    #[doc = " Low-impact concern."]
    Low,
    #[doc = " Material but non-release-blocking concern."]
    Medium,
    #[doc = " High-impact concern."]
    High,
    #[doc = " Critical concern."]
    Critical,
}
#[doc = " Current explicit finding disposition."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FindingDisposition {
    #[doc = " Finding remains open."]
    Open,
    #[doc = " Finding was resolved with retained evidence."]
    Resolved,
    #[doc = " Finding requests an independent waiver."]
    WaiverRequested,
    #[doc = " Finding was ignored; this always blocks release."]
    Ignored,
    #[doc = " Finding was quarantined; this always blocks release."]
    Quarantined,
}
#[doc = " One current finding state bound to the exact candidate."]
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
    #[doc = " Creates one signed finding-state observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for a placeholder finding digest."]
    pub fn new(
        id: FindingId,
        binding: EvidenceBinding,
        reporter: PrincipalId,
        severity: FindingSeverity,
        release_blocking: bool,
        disposition: FindingDisposition,
        finding_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::validation::require_digest(finding_digest)?;
        Ok(Self { id, binding, reporter, severity, release_blocking, disposition, finding_digest })
    }
    #[doc = " Returns the stable finding identity."]
    #[must_use]
    pub const fn id(&self) -> FindingId {
        self.id
    }
    #[doc = " Returns the exact evidence binding."]
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding {
        self.binding
    }
    #[doc = " Returns the finding reporter."]
    #[must_use]
    pub const fn reporter(&self) -> PrincipalId {
        self.reporter
    }
    #[doc = " Returns the severity."]
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }
    #[doc = " Returns whether policy marked the finding release-blocking."]
    #[must_use]
    pub const fn release_blocking(&self) -> bool {
        self.release_blocking
    }
    #[doc = " Returns the current disposition."]
    #[must_use]
    pub const fn disposition(&self) -> FindingDisposition {
        self.disposition
    }
    #[doc = " Returns the signed finding-state digest."]
    #[must_use]
    pub const fn finding_digest(&self) -> Sha256Digest {
        self.finding_digest
    }
}
