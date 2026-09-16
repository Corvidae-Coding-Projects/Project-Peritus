//! Independent finding-waiver observations.

use crate::{ConstructionError, EvidenceBinding, FindingId, PrincipalId, ReleaseCandidate};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Explicit independent waiver for one exact non-release-blocking finding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverObservation {
    finding_id: FindingId,
    binding: EvidenceBinding,
    authority: PrincipalId,
    waiver_digest: Sha256Digest,
    justification_digest: Sha256Digest,
    approved: bool,
}

impl WaiverObservation {
    /// Creates one signed waiver observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder waiver or justification digest.
    pub fn new(
        finding_id: FindingId,
        binding: EvidenceBinding,
        authority: PrincipalId,
        waiver_digest: Sha256Digest,
        justification_digest: Sha256Digest,
        approved: bool,
    ) -> (result: Result<Self, ConstructionError>)
        ensures
            result.is_ok() == (crate::validation::spec_digest_nonzero(waiver_digest)
                && crate::validation::spec_digest_nonzero(justification_digest)),
            match result {
                Ok(value) => value.spec_finding_id() == finding_id
                    && value.spec_binding() == binding
                    && value.spec_authority() == authority
                    && value.spec_waiver_digest() == waiver_digest
                    && value.spec_justification_digest() == justification_digest
                    && value.spec_approved() == approved,
                Err(error) => error.spec_kind() == crate::ConstructionErrorKind::ZeroDigest,
            },
    {
        crate::validation::require_digest(waiver_digest)?;
        crate::validation::require_digest(justification_digest)?;
        Ok(Self {
            finding_id,
            binding,
            authority,
            waiver_digest,
            justification_digest,
            approved,
        })
    }

    /// Returns the exact finding identity.
    #[must_use]
    pub const fn finding_id(&self) -> (id: FindingId) ensures id == self.spec_finding_id() {
        self.finding_id
    }

    /// Returns the exact candidate/time/sequence/revision binding.
    #[must_use]
    pub const fn binding(&self) -> (binding: EvidenceBinding)
        ensures binding == self.spec_binding()
    {
        self.binding
    }

    /// Returns the admitted waiver authority.
    #[must_use]
    pub const fn authority(&self) -> (authority: PrincipalId)
        ensures authority == self.spec_authority()
    {
        self.authority
    }

    /// Returns the signed waiver digest.
    #[must_use]
    pub const fn waiver_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_waiver_digest()
    {
        self.waiver_digest
    }

    /// Returns the retained justification digest.
    #[must_use]
    pub const fn justification_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_justification_digest()
    {
        self.justification_digest
    }

    /// Returns whether authority approved the waiver.
    #[must_use]
    pub const fn approved(&self) -> (approved: bool) ensures approved == self.spec_approved() {
        self.approved
    }

    /// Returns whether the waiver is exact and current for the evaluation candidate.
    #[must_use]
    pub const fn is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> (current: bool)
        ensures current == self.spec_is_current_for(candidate, evaluated_at),
    {
        let current = self.binding().is_current_for(candidate, evaluated_at);
        proof {
            reveal(WaiverObservation::spec_is_current_for);
        }
        current
    }

    /// Logical view of the exact finding identity.
    pub closed spec fn spec_finding_id(&self) -> FindingId { self.finding_id }

    /// Logical view of the exact evidence binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }

    /// Logical view of the admitted waiver authority.
    pub closed spec fn spec_authority(&self) -> PrincipalId { self.authority }

    /// Logical view of the signed waiver digest.
    pub closed spec fn spec_waiver_digest(&self) -> Sha256Digest { self.waiver_digest }

    /// Logical view of the retained justification digest.
    pub closed spec fn spec_justification_digest(&self) -> Sha256Digest {
        self.justification_digest
    }

    /// Logical view of whether authority approved the waiver.
    pub closed spec fn spec_approved(&self) -> bool { self.approved }

    /// Logical view of exact-current waiver binding.
    pub open spec fn spec_is_current_for(
        &self,
        candidate: ReleaseCandidate,
        evaluated_at: u64,
    ) -> bool {
        self.spec_binding().spec_is_current_for(candidate, evaluated_at)
    }

    /// Logical equality of every supplied waiver field.
    pub open spec fn spec_matches(&self, other: WaiverObservation) -> bool {
        crate::identity::finding_ids_match(self.spec_finding_id(), other.spec_finding_id())
            && crate::evidence::bindings_match(self.spec_binding(), other.spec_binding())
            && crate::identity::principal_ids_match(
                self.spec_authority(),
                other.spec_authority(),
            )
            && crate::candidate::digest_matches(
                self.spec_waiver_digest(),
                other.spec_waiver_digest(),
            )
            && crate::candidate::digest_matches(
                self.spec_justification_digest(),
                other.spec_justification_digest(),
            )
            && self.spec_approved() == other.spec_approved()
    }
}

} // verus!
