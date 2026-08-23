//! Commands that fence an active lease generation.

use crate::{HolderLossEvidence, LeaseClaim};
use peritus_policy::AuthorityInstant;
use peritus_types::{CommandId, EvidenceId};
use vstd::prelude::*;

verus! {

/// Fences an active claim whose exclusive deadline has arrived.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExpireLease {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
}

impl ExpireLease {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the authority-time observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Creates an expiry plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_observed_at() == observed_at,
    {
        Self { command_id, observed_at }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures
            result == self.spec_command_id(),
            result == self.command_id,
    { self.command_id }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures
            result == self.spec_observed_at(),
            result == self.observed_at,
    { self.observed_at }
}

/// Fences an active claim after exact holder-loss evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FenceHolderLoss {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) evidence: HolderLossEvidence,
}

impl FenceHolderLoss {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the authority-time observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Returns the exact holder-loss evidence used by specifications.
    pub closed spec fn spec_evidence(&self) -> HolderLossEvidence { self.evidence }

    /// Creates a holder-loss fencing plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
        evidence: HolderLossEvidence,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_observed_at() == observed_at,
            command.spec_evidence() == evidence,
    {
        Self { command_id, observed_at, evidence }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures
            result == self.spec_command_id(),
            result == self.command_id,
    { self.command_id }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures
            result == self.spec_observed_at(),
            result == self.observed_at,
    { self.observed_at }
    pub(crate) const fn evidence(self) -> (result: HolderLossEvidence)
        ensures result == self.evidence,
    { self.evidence }
}

/// Explicitly fences an active claim after an authority-clock discontinuity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FenceClockDiscontinuity {
    pub(crate) command_id: CommandId,
    pub(crate) observed_at: AuthorityInstant,
}

impl FenceClockDiscontinuity {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the authority-time observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Creates a discontinuity fencing plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        observed_at: AuthorityInstant,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_observed_at() == observed_at,
    {
        Self { command_id, observed_at }
    }

    pub(crate) const fn command_id(self) -> (result: CommandId)
        ensures
            result == self.spec_command_id(),
            result == self.command_id,
    { self.command_id }
    pub(crate) const fn observed_at(self) -> (result: AuthorityInstant)
        ensures
            result == self.spec_observed_at(),
            result == self.observed_at,
    { self.observed_at }
}

/// Fences an active claim after a separately authorized revocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RevokeLease {
    pub(crate) command_id: CommandId,
    pub(crate) claim: LeaseClaim,
    pub(crate) observed_at: AuthorityInstant,
    pub(crate) evidence_id: EvidenceId,
}

impl RevokeLease {
    /// Returns the command identity used by specifications.
    pub closed spec fn spec_command_id(&self) -> CommandId { self.command_id }

    /// Returns the exact claim used by specifications.
    pub closed spec fn spec_claim(&self) -> LeaseClaim { self.claim }

    /// Returns the authority-time observation used by specifications.
    pub closed spec fn spec_observed_at(&self) -> AuthorityInstant { self.observed_at }

    /// Returns the exact revocation evidence identity used by specifications.
    pub closed spec fn spec_evidence_id(&self) -> EvidenceId { self.evidence_id }

    /// Creates an unprivileged logical revocation plan.
    #[must_use]
    pub const fn new(
        command_id: CommandId,
        claim: LeaseClaim,
        observed_at: AuthorityInstant,
        evidence_id: EvidenceId,
    ) -> (command: Self)
        ensures
            command.spec_command_id() == command_id,
            command.spec_claim() == claim,
            command.spec_observed_at() == observed_at,
            command.spec_evidence_id() == evidence_id,
    {
        Self { command_id, claim, observed_at, evidence_id }
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
    pub(crate) const fn evidence_id(self) -> (result: EvidenceId)
        ensures result == self.evidence_id,
    { self.evidence_id }
}

} // verus!
