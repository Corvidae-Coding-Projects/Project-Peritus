//! Commands that fence an active lease generation.

use crate::{HolderLossEvidence, LeaseClaim};
use peritus_policy::AuthorityInstant;
use peritus_types::{CommandId, EvidenceId};

#[doc = " Fences an active claim whose exclusive deadline has arrived."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExpireLease {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
}
impl ExpireLease {
    #[doc = " Creates an expiry plan."]
    #[must_use]
    pub const fn new(command_id: CommandId, observed_at: AuthorityInstant) -> Self {
        Self { command_id, observed_at }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
#[doc = " Fences an active claim after exact holder-loss evidence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FenceHolderLoss {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) evidence: HolderLossEvidence,
}
impl FenceHolderLoss {
    #[doc = " Creates a holder-loss fencing plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
        evidence: HolderLossEvidence,
    ) -> Self {
        Self { command_id, observed_at, evidence }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
    pub(crate) const fn evidence(self) -> HolderLossEvidence {
        self.evidence
    }
}
#[doc = " Explicitly fences an active claim after an authority-clock discontinuity."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FenceClockDiscontinuity {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
}
impl FenceClockDiscontinuity {
    #[doc = " Creates a discontinuity fencing plan."]
    #[must_use]
    pub const fn new(command_id: CommandId, observed_at: AuthorityInstant) -> Self {
        Self { command_id, observed_at }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
#[doc = " Fences an active claim after a separately authorized revocation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RevokeLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) evidence_id: EvidenceId,
}
impl RevokeLease {
    #[doc = " Creates an unprivileged logical revocation plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        evidence_id: EvidenceId,
    ) -> Self {
        Self { command_id, claim, observed_at, evidence_id }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn claim(self) -> LeaseClaim {
        self.claim
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
    pub(crate) const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
}
