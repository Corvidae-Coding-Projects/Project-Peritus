//! Exact fencing evidence and post-fence reconciliation observations.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use crate::{LeaseClaim, LeaseHolder, LeaseScope};
use peritus_types::{EvidenceId, Generation};
use vstd::prelude::*;

verus! {

/// Reason the prior active generation was fenced.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FenceCause {
    /// A holder released without exact quiescence evidence.
    ReleasedWithoutQuiescence,
    /// The half-open lease validity interval ended.
    Expired,
    /// Exact external evidence reported holder disappearance.
    HolderLost,
    /// Authority time changed epoch or regressed.
    ClockDiscontinuity,
    /// An authorized revocation requested fencing.
    Revoked,
}

/// Unprivileged evidence claiming an active holder disappeared.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HolderLossEvidence {
    claim: LeaseClaim,
    evidence_id: EvidenceId,
}

impl HolderLossEvidence {
    /// Returns the exact correlated claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }

    /// Returns the exact evidence identity used by specifications.
    pub closed spec fn spec_evidence_id(&self) -> EvidenceId { self.evidence_id }

    /// Creates exact raw holder-loss evidence.
    #[must_use]
    pub const fn new(claim: LeaseClaim, evidence_id: EvidenceId) -> Self {
        Self { claim, evidence_id }
    }

    /// Returns the correlated lease claim.
    #[must_use]
    pub const fn claim(self) -> (claim: LeaseClaim)
        ensures claim == self.spec_claim(),
    { self.claim }

    /// Returns the external evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> (evidence_id: EvidenceId)
        ensures evidence_id == self.spec_evidence_id(),
    { self.evidence_id }
}

/// Unprivileged evidence claiming the exact holder is quiescent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HolderQuiescenceEvidence {
    claim: LeaseClaim,
    evidence_id: EvidenceId,
}

impl HolderQuiescenceEvidence {
    /// Returns the exact correlated claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }

    /// Returns the exact evidence identity used by specifications.
    pub closed spec fn spec_evidence_id(&self) -> EvidenceId { self.evidence_id }

    /// Creates exact raw holder-quiescence evidence.
    #[must_use]
    pub const fn new(claim: LeaseClaim, evidence_id: EvidenceId) -> Self {
        Self { claim, evidence_id }
    }

    /// Returns the correlated lease claim.
    #[must_use]
    pub const fn claim(self) -> (claim: LeaseClaim)
        ensures claim == self.spec_claim(),
    { self.claim }

    /// Returns the external evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> (evidence_id: EvidenceId)
        ensures evidence_id == self.spec_evidence_id(),
    { self.evidence_id }
}

/// Exact lineage facts that every reconciliation observation must echo.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationCorrelation {
    pub(crate) scope: LeaseScope,
    pub(crate) fenced_generation: Generation,
    pub(crate) prior_holder: LeaseHolder,
}

impl ReconciliationCorrelation {
    /// Returns the exact scope used by specifications.
    pub closed spec fn spec_scope(&self) -> LeaseScope { self.scope }

    /// Returns the exact fenced generation used by specifications.
    pub closed spec fn spec_fenced_generation(&self) -> Generation { self.fenced_generation }

    /// Returns the exact prior holder used by specifications.
    pub closed spec fn spec_prior_holder(&self) -> LeaseHolder { self.prior_holder }

    pub(crate) proof fn reveal_exact_fields(&self)
        ensures
            self.spec_scope() == self.scope,
            self.spec_fenced_generation() == self.fenced_generation,
            self.spec_prior_holder() == self.prior_holder,
    {
    }

    /// Creates raw correlation facts returned by a downstream inspector.
    ///
    /// Construction is intentionally unprivileged; the reducer compares every field against the
    /// pending fenced generation before accepting any disposition.
    #[must_use]
    pub const fn new(
        scope: LeaseScope,
        fenced_generation: Generation,
        prior_holder: LeaseHolder,
    ) -> (result: Self)
        ensures
            result.spec_scope() == scope,
            result.spec_fenced_generation() == fenced_generation,
            result.spec_prior_holder() == prior_holder,
    {
        Self { scope, fenced_generation, prior_holder }
    }

    /// Returns the exact lease scope.
    #[must_use]
    pub const fn scope(self) -> (scope: LeaseScope)
        ensures scope == self.spec_scope(),
    { self.scope }

    /// Returns the generation invalidated by fencing.
    #[must_use]
    pub const fn fenced_generation(self) -> (generation: Generation)
        ensures generation == self.spec_fenced_generation(),
    { self.fenced_generation }

    /// Returns the prior actor/session holder.
    #[must_use]
    pub const fn prior_holder(self) -> (holder: LeaseHolder)
        ensures holder == self.spec_prior_holder(),
    { self.prior_holder }
}

/// Safety result claimed by the downstream resource/holder inspectors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReconciliationDisposition {
    /// Exact evidence claims the holder is quiescent and resource is safe.
    SafeToAcquire {
        /// Holder-quiescence evidence identity.
        holder_quiescence: EvidenceId,
        /// Resource-safety evidence identity.
        resource_safety: EvidenceId,
    },
    /// Exact evidence reports a dirty resource.
    Dirty {
        /// Dirty-resource evidence identity.
        evidence_id: EvidenceId,
    },
    /// Exact evidence cannot determine safety.
    Indeterminate {
        /// Indeterminate inspection evidence identity.
        evidence_id: EvidenceId,
    },
}

/// Raw, unprivileged, exactly correlated reconciliation observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationObservation {
    pub(crate) correlation: ReconciliationCorrelation,
    pub(crate) disposition: ReconciliationDisposition,
}

impl ReconciliationObservation {
    /// Creates a raw downstream observation.
    #[must_use]
    pub const fn new(
        correlation: ReconciliationCorrelation,
        disposition: ReconciliationDisposition,
    ) -> Self {
        Self { correlation, disposition }
    }

    /// Returns the exact echoed correlation.
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation { self.correlation }

    /// Returns the claimed safety result.
    #[must_use]
    pub const fn disposition(self) -> ReconciliationDisposition { self.disposition }
}

} // verus!
