//! Checked logical lease command values.

use crate::{
    HolderQuiescenceEvidence, LeaseClaim, LeaseDuration, LeaseHolder, LeaseScope,
    ReconciliationObservation,
};
use peritus_policy::AuthorityInstant;
use peritus_policy::CapabilityUseTransition;
use peritus_types::CommandId;
use vstd::prelude::*;

verus! {

mod fencing;

pub use fencing::{ExpireLease, FenceClockDiscontinuity, FenceHolderLoss, RevokeLease};

/// Mints one new aggregate at generation and version one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MintLease {
    pub(crate) command_id: CommandId,
    pub(crate) scope: LeaseScope,
    pub(crate) observed_at: AuthorityInstant,
}

impl MintLease {
    /// Returns the exact command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact scope used by specifications.
    pub closed spec fn spec_scope(&self) -> LeaseScope { self.scope }
    /// Returns the exact observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Creates a mint plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        scope: LeaseScope,
        observed_at: AuthorityInstant,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_scope() == scope,
            command.spec_observed_at() == observed_at,
    {
        Self { command_id, scope, observed_at }
    }

    pub(crate) const fn scope(self) -> (result: LeaseScope)
        ensures result == self.scope,
    { self.scope }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures result == self.observed_at,
    { self.observed_at }
}

/// Acquires one currently available lease.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AcquireLease {
    pub(crate) command_id: CommandId,
    pub(crate) holder: LeaseHolder,
    pub(crate) duration: LeaseDuration,
    pub(crate) observed_at: AuthorityInstant,
}

impl AcquireLease {
    /// Returns the exact command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact holder used by specifications.
    pub closed spec fn spec_holder(&self) -> LeaseHolder { self.holder }
    /// Returns the exact duration used by specifications.
    pub closed spec fn spec_duration(&self) -> LeaseDuration { self.duration }
    /// Returns the exact observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Creates an acquisition plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        holder: LeaseHolder,
        duration: LeaseDuration,
        observed_at: AuthorityInstant,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_holder() == holder,
            command.spec_duration() == duration,
            command.spec_observed_at() == observed_at,
    {
        Self { command_id, holder, duration, observed_at }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures result == self.command_id,
    { self.command_id }
    pub(crate) const fn holder(self) -> (result: LeaseHolder)
        ensures result == self.holder,
    { self.holder }
    pub(crate) const fn duration(self) -> (result: LeaseDuration)
        ensures result == self.duration,
    { self.duration }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures result == self.observed_at,
    { self.observed_at }
}

/// Renews one exact active claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RenewLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) duration: LeaseDuration,
    pub(crate) observed_at: AuthorityInstant,
}

/// Intersects one exact active claim with one freshly consumed policy capability.
///
/// This command is move-only because it owns the move-only policy logical-use transition.
///
/// ```compile_fail
/// use peritus_leases::UseLease;
/// fn require_clone<T: Clone>() {}
/// require_clone::<UseLease>();
/// ```
pub struct UseLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) capability_use: CapabilityUseTransition,
}

impl UseLease {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the exact claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }

    /// Returns the exact authority observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Returns the move-only policy transition used by specifications.
    pub closed spec fn spec_capability_use(&self) -> CapabilityUseTransition {
        self.capability_use
    }

    pub(crate) proof fn reveal_exact_fields(&self)
        ensures
            self.spec_command_id() == self.command_id,
            self.spec_claim() == self.claim,
            self.spec_observed_at() == self.observed_at,
            self.spec_capability_use() == self.capability_use,
    {
    }

    /// Creates an exact logical lease-use plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        capability_use: CapabilityUseTransition,
    ) -> (result: Self)
        ensures
            result.spec_command_id() == command_id,
            result.spec_claim() == claim,
            result.spec_observed_at() == observed_at,
            result.spec_capability_use() == capability_use,
    {
        Self { command_id, claim, observed_at, capability_use }
    }

    /// Returns the exact idempotency identity.
    #[must_use]
    pub const fn command_id(&self) -> (result: CommandId)
        ensures result == self.spec_command_id(),
    { self.command_id }
    /// Returns the exact unprivileged lease claim.
    #[must_use]
    pub const fn claim(&self) -> (result: LeaseClaim)
        ensures result == self.spec_claim(),
    { self.claim }
    /// Returns the exact authority-clock observation.
    #[must_use]
    pub const fn observed_at(&self) -> (result: AuthorityInstant)
        ensures result == self.spec_observed_at(),
    { self.observed_at }

    /// Borrows the exact move-only policy logical-use transition.
    #[must_use]
    pub const fn capability_use(&self) -> (result: &CapabilityUseTransition)
        ensures *result == self.spec_capability_use(),
    {
        &self.capability_use
    }

    pub(crate) fn into_parts(
        self,
    ) -> (result: (CommandId, LeaseClaim, AuthorityInstant, CapabilityUseTransition))
        ensures
            result.0 == self.command_id,
            result.1 == self.claim,
            result.2 == self.observed_at,
            result.3 == self.capability_use,
    {
        (self.command_id, self.claim, self.observed_at, self.capability_use)
    }
}

impl RenewLease {
    /// Returns the exact command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }
    /// Returns the exact duration used by specifications.
    pub closed spec fn spec_duration(&self) -> LeaseDuration { self.duration }
    /// Returns the exact observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Creates a renewal plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        duration: LeaseDuration,
        observed_at: AuthorityInstant,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_claim() == claim,
            command.spec_duration() == duration,
            command.spec_observed_at() == observed_at,
    {
        Self { command_id, claim, duration, observed_at }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures result == self.command_id,
    { self.command_id }
    pub(crate) const fn claim(self) -> (result: LeaseClaim)
        ensures result == self.claim,
    { self.claim }
    pub(crate) const fn duration(self) -> (result: LeaseDuration)
        ensures result == self.duration,
    { self.duration }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures result == self.observed_at,
    { self.observed_at }
}

/// Voluntarily releases one exact active claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReleaseLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) quiescence: Option<HolderQuiescenceEvidence>,
}

impl ReleaseLease {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the exact claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }

    /// Returns the authority-time observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Returns the exact optional quiescence evidence used by specifications.
    pub closed spec fn spec_quiescence(&self) -> Option<HolderQuiescenceEvidence> {
        self.quiescence
    }

    /// Creates a release plan. Missing quiescence evidence enters reconciliation.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        quiescence: Option<HolderQuiescenceEvidence>,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_claim() == claim,
            command.spec_observed_at() == observed_at,
            command.spec_quiescence() == quiescence,
    {
        Self { command_id, claim, observed_at, quiescence }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures
            result == self.spec_command_id(),
            result == self.command_id,
    { self.command_id }
    pub(crate) const fn claim(self) -> (result: LeaseClaim)
        ensures result == self.claim,
    { self.claim }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures
            result == self.spec_observed_at(),
            result == self.observed_at,
    { self.observed_at }
    pub(crate) const fn quiescence(self) -> (result: Option<HolderQuiescenceEvidence>)
        ensures
            result == self.spec_quiescence(),
            result == self.quiescence,
    { self.quiescence }
}

/// Resolves one exactly correlated fenced generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReconcileLease {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) observation: ReconciliationObservation,
}

impl ReconcileLease {
    /// Returns the exact command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }
    /// Returns the exact observation time used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }
    /// Returns the exact reconciliation observation used by specifications.
    pub closed spec fn spec_observation(&self) -> ReconciliationObservation { self.observation }

    /// Creates a reconciliation plan from a raw observation.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
        observation: ReconciliationObservation,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_observed_at() == observed_at,
            command.spec_observation() == observation,
    {
        Self { command_id, observed_at, observation }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures result == self.command_id,
    { self.command_id }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures result == self.observed_at,
    { self.observed_at }
}

} // verus!
