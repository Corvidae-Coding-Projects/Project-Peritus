//! Checked logical lease command values.

use crate::{
    HolderQuiescenceEvidence, LeaseClaim, LeaseDuration, LeaseHolder, LeaseScope,
    ReconciliationObservation,
};
use peritus_policy::AuthorityInstant;
use peritus_policy::CapabilityUseTransition;
use peritus_types::CommandId;

mod fencing;
pub use fencing::{ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, RevokeLease};
#[doc = " Mints one new aggregate at generation and version one."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MintLease {
    pub(crate) command_id: CommandId,
    pub(crate) scope: LeaseScope,
    pub(crate) observed_at: AuthorityInstant,
}
impl MintLease {
    #[doc = " Creates a mint plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        scope: LeaseScope,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self { command_id, scope, observed_at }
    }
    pub(crate) const fn scope(self) -> LeaseScope {
        self.scope
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
#[doc = " Acquires one currently available lease."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AcquireLease {
    pub(crate) command_id: CommandId,
    pub(crate) holder: LeaseHolder,
    pub(crate) duration: LeaseDuration,
    pub(crate) observed_at: AuthorityInstant,
}
impl AcquireLease {
    #[doc = " Creates an acquisition plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        holder: LeaseHolder,
        duration: LeaseDuration,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self { command_id, holder, duration, observed_at }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn holder(self) -> LeaseHolder {
        self.holder
    }
    pub(crate) const fn duration(self) -> LeaseDuration {
        self.duration
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
#[doc = " Renews one exact active claim."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenewLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) duration: LeaseDuration,
    pub(crate) observed_at: AuthorityInstant,
}
#[doc = " Intersects one exact active claim with one freshly consumed policy capability."]
#[doc = ""]
#[doc = " This command is move-only because it owns the move-only policy logical-use transition."]
#[doc = ""]
#[doc = " ```compile_fail"]
#[doc = " use peritus_leases::UseLease;"]
#[doc = " fn require_clone<T: Clone>() {}"]
#[doc = " require_clone::<UseLease>();"]
#[doc = " ```"]
pub struct UseLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) capability_use: CapabilityUseTransition,
}
impl UseLease {
    #[doc = " Creates an exact logical lease-use plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        capability_use: CapabilityUseTransition,
    ) -> Self {
        Self { command_id, claim, observed_at, capability_use }
    }
    #[doc = " Returns the exact idempotency identity."]
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    #[doc = " Returns the exact unprivileged lease claim."]
    #[must_use]
    pub const fn claim(&self) -> LeaseClaim {
        self.claim
    }
    #[doc = " Returns the exact authority-clock observation."]
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    #[doc = " Borrows the exact move-only policy logical-use transition."]
    #[must_use]
    pub const fn capability_use(&self) -> &CapabilityUseTransition {
        &self.capability_use
    }
    pub(crate) fn into_parts(
        self,
    ) -> (CommandId, LeaseClaim, AuthorityInstant, CapabilityUseTransition) {
        (self.command_id, self.claim, self.observed_at, self.capability_use)
    }
}
impl RenewLease {
    #[doc = " Creates a renewal plan."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        duration: LeaseDuration,
        observed_at: AuthorityInstant,
    ) -> Self {
        Self { command_id, claim, duration, observed_at }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn claim(self) -> LeaseClaim {
        self.claim
    }
    pub(crate) const fn duration(self) -> LeaseDuration {
        self.duration
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
#[doc = " Voluntarily releases one exact active claim."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) quiescence: Option<HolderQuiescenceEvidence>,
}
impl ReleaseLease {
    #[doc = " Creates a release plan. Missing quiescence evidence enters reconciliation."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        quiescence: Option<HolderQuiescenceEvidence>,
    ) -> Self {
        Self { command_id, claim, observed_at, quiescence }
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
    pub(crate) const fn quiescence(self) -> Option<HolderQuiescenceEvidence> {
        self.quiescence
    }
}
#[doc = " Resolves one exactly correlated fenced generation."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconcileLease {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) observation: ReconciliationObservation,
}
impl ReconcileLease {
    #[doc = " Creates a reconciliation plan from a raw observation."]
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
        observation: ReconciliationObservation,
    ) -> Self {
        Self { command_id, observed_at, observation }
    }
    pub(crate) const fn command_id(self) -> CommandId {
        self.command_id
    }
    pub(crate) const fn observed_at(self) -> AuthorityInstant {
        self.observed_at
    }
}
