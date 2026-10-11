//! Exact fencing evidence and post-fence reconciliation observations.
#![allow(
    missing_docs,
    reason = "pinned Cargo-Verus synthesizes undocumented accessors for documented payload variants"
)]

use crate::{LeaseClaim, LeaseHolder, LeaseScope};
use peritus_types::{EvidenceId, Generation};

#[doc = " Reason the prior active generation was fenced."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FenceCause {
    #[doc = " A holder released without exact quiescence evidence."]
    ReleasedWithoutQuiescence,
    #[doc = " The half-open lease validity interval ended."]
    Expired,
    #[doc = " Exact external evidence reported holder disappearance."]
    HolderLost,
    #[doc = " Authority time changed epoch or regressed."]
    ClockDiscontinuity,
    #[doc = " An authorized revocation requested fencing."]
    Revoked,
}
#[doc = " Unprivileged evidence claiming an active holder disappeared."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HolderLossEvidence {
    claim: LeaseClaim,
    evidence_id: EvidenceId,
}
impl HolderLossEvidence {
    #[doc = " Creates exact raw holder-loss evidence."]
    #[must_use]
    pub const fn new(claim: LeaseClaim, evidence_id: EvidenceId) -> Self {
        Self { claim, evidence_id }
    }
    #[doc = " Returns the correlated lease claim."]
    #[must_use]
    pub const fn claim(self) -> LeaseClaim {
        self.claim
    }
    #[doc = " Returns the external evidence identity."]
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
}
#[doc = " Unprivileged evidence claiming the exact holder is quiescent."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HolderQuiescenceEvidence {
    claim: LeaseClaim,
    evidence_id: EvidenceId,
}
impl HolderQuiescenceEvidence {
    #[doc = " Creates exact raw holder-quiescence evidence."]
    #[must_use]
    pub const fn new(claim: LeaseClaim, evidence_id: EvidenceId) -> Self {
        Self { claim, evidence_id }
    }
    #[doc = " Returns the correlated lease claim."]
    #[must_use]
    pub const fn claim(self) -> LeaseClaim {
        self.claim
    }
    #[doc = " Returns the external evidence identity."]
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
}
#[doc = " Exact lineage facts that every reconciliation observation must echo."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationCorrelation {
    pub(crate) scope: LeaseScope,
    pub(crate) fenced_generation: Generation,
    pub(crate) prior_holder: LeaseHolder,
}
impl ReconciliationCorrelation {
    #[doc = " Creates raw correlation facts returned by a downstream inspector."]
    #[doc = ""]
    #[doc = " Construction is intentionally unprivileged; the reducer compares every field against the"]
    #[doc = " pending fenced generation before accepting any disposition."]
    #[must_use]
    pub const fn new(
        scope: LeaseScope,
        fenced_generation: Generation,
        prior_holder: LeaseHolder,
    ) -> Self {
        Self { scope, fenced_generation, prior_holder }
    }
    #[doc = " Returns the exact lease scope."]
    #[must_use]
    pub const fn scope(self) -> LeaseScope {
        self.scope
    }
    #[doc = " Returns the generation invalidated by fencing."]
    #[must_use]
    pub const fn fenced_generation(self) -> Generation {
        self.fenced_generation
    }
    #[doc = " Returns the prior actor/session holder."]
    #[must_use]
    pub const fn prior_holder(self) -> LeaseHolder {
        self.prior_holder
    }
}
#[doc = " Safety result claimed by the downstream resource/holder inspectors."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReconciliationDisposition {
    #[doc = " Exact evidence claims the holder is quiescent and resource is safe."]
    SafeToAcquire {
        #[doc = " Holder-quiescence evidence identity."]
        holder_quiescence: EvidenceId,
        #[doc = " Resource-safety evidence identity."]
        resource_safety: EvidenceId,
    },
    #[doc = " Exact evidence reports a dirty resource."]
    Dirty {
        #[doc = " Dirty-resource evidence identity."]
        evidence_id: EvidenceId,
    },
    #[doc = " Exact evidence cannot determine safety."]
    Indeterminate {
        #[doc = " Indeterminate inspection evidence identity."]
        evidence_id: EvidenceId,
    },
}
#[doc = " Raw, unprivileged, exactly correlated reconciliation observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconciliationObservation {
    pub(crate) correlation: ReconciliationCorrelation,
    pub(crate) disposition: ReconciliationDisposition,
}
impl ReconciliationObservation {
    #[doc = " Creates a raw downstream observation."]
    #[must_use]
    pub const fn new(
        correlation: ReconciliationCorrelation,
        disposition: ReconciliationDisposition,
    ) -> Self {
        Self { correlation, disposition }
    }
    #[doc = " Returns the exact echoed correlation."]
    #[must_use]
    pub const fn correlation(self) -> ReconciliationCorrelation {
        self.correlation
    }
    #[doc = " Returns the claimed safety result."]
    #[must_use]
    pub const fn disposition(self) -> ReconciliationDisposition {
        self.disposition
    }
}
