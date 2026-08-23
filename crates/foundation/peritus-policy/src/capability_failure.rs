//! Rejected capability uses that preserve the move-only authority value.

use crate::{Capability, PolicyError};
#[cfg(verus_only)]
use crate::{ActorRole, Permission};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Rejected capability-use transition that preserves ownership of the unchanged capability.
#[derive(Debug, Eq, PartialEq)]
pub struct CapabilityUseFailure {
    error: PolicyError,
    capability: Capability,
}

impl CapabilityUseFailure {
    /// Returns the exact rejected error category used by specifications.
    pub closed spec fn spec_error_kind(&self) -> crate::PolicyErrorKind {
        self.error.spec_kind()
    }

    /// Returns the exact rejected scope dimension used by specifications.
    pub closed spec fn spec_error_dimension(&self) -> Option<crate::ScopeDimension> {
        self.error.spec_dimension()
    }

    /// Returns the exact rejected collection detail used by specifications.
    pub closed spec fn spec_error_collection(&self) -> Option<crate::CanonicalCollection> {
        self.error.spec_collection()
    }

    /// Returns the unchanged actor identity bytes used by specifications.
    pub closed spec fn spec_scope_actor_id(&self) -> [u8; 16] {
        self.capability.spec_scope_actor_id()
    }

    /// Returns the unchanged stable role used by specifications.
    pub closed spec fn spec_scope_role(&self) -> ActorRole {
        self.capability.spec_scope_role()
    }

    /// Returns the unchanged environment identity bytes used by specifications.
    pub closed spec fn spec_scope_environment_id(&self) -> [u8; 16] {
        self.capability.spec_scope_environment_id()
    }

    /// Returns the unchanged ordered permission sequence used by specifications.
    pub closed spec fn spec_scope_permissions(&self) -> Seq<Permission> {
        self.capability.spec_scope_permissions()
    }

    /// Returns the unchanged revision tuple used by specifications.
    pub closed spec fn spec_scope_revision(&self) -> RevisionTuple {
        self.capability.spec_scope_revision()
    }

    /// Returns the unchanged validity window used by specifications.
    pub closed spec fn spec_scope_validity(&self) -> crate::ValidityWindow {
        self.capability.spec_scope_validity()
    }

    /// Returns the unchanged immutable scope use limit used by specifications.
    pub closed spec fn spec_scope_use_limit(&self) -> Option<int> {
        self.capability.spec_scope_use_limit()
    }

    /// Returns the unchanged remaining-use bound used by specifications.
    pub closed spec fn spec_remaining_uses(&self) -> Option<int> {
        self.capability.spec_remaining_uses()
    }

    /// Returns the unchanged issuance instant used by specifications.
    pub closed spec fn spec_issued_at(&self) -> crate::AuthorityInstant {
        self.capability.spec_issued_at()
    }

    /// Returns the unchanged issuance digest used by specifications.
    pub closed spec fn spec_issuance_digest(&self) -> [u8; 32] {
        self.capability.spec_issuance_digest()
    }

    /// Returns the unchanged issuance command identity used by specifications.
    pub closed spec fn spec_issuance_command_id(&self) -> [u8; 16] {
        self.capability.spec_issuance_command_id()
    }

    /// Returns the unchanged authority-time epoch used by specifications.
    pub closed spec fn spec_time_epoch(&self) -> int { self.capability.spec_time_epoch() }

    /// Returns the unchanged greatest authority-time tick used by specifications.
    pub closed spec fn spec_greatest_tick(&self) -> int {
        self.capability.spec_greatest_tick()
    }

    pub(crate) const fn new(error: PolicyError, capability: Capability) -> (failure: Self)
        ensures
            failure.spec_error_kind() == error.spec_kind(),
            failure.spec_error_dimension() == error.spec_dimension(),
            failure.spec_error_collection() == error.spec_collection(),
            failure.spec_scope_actor_id() == capability.spec_scope_actor_id(),
            failure.spec_scope_role() == capability.spec_scope_role(),
            failure.spec_scope_environment_id() == capability.spec_scope_environment_id(),
            failure.spec_scope_permissions() == capability.spec_scope_permissions(),
            failure.spec_scope_revision() == capability.spec_scope_revision(),
            failure.spec_scope_validity() == capability.spec_scope_validity(),
            failure.spec_scope_use_limit() == capability.spec_scope_use_limit(),
            failure.spec_remaining_uses() == capability.spec_remaining_uses(),
            failure.spec_issued_at() == capability.spec_issued_at(),
            failure.spec_issuance_digest() == capability.spec_issuance_digest(),
            failure.spec_issuance_command_id() == capability.spec_issuance_command_id(),
            failure.spec_time_epoch() == capability.spec_time_epoch(),
            failure.spec_greatest_tick() == capability.spec_greatest_tick(),
    {
        Self { error, capability }
    }

    /// Returns the exact typed failure.
    #[must_use]
    pub const fn error(&self) -> (error: PolicyError)
        ensures
            error.spec_kind() == self.spec_error_kind(),
            error.spec_dimension() == self.spec_error_dimension(),
            error.spec_collection() == self.spec_error_collection(),
    { self.error }

    /// Borrows the unchanged prior capability.
    #[must_use]
    pub const fn capability(&self) -> &Capability { &self.capability }

    /// Consumes the failure and returns the unchanged prior capability.
    #[must_use]
    pub fn into_capability(self) -> Capability { self.capability }
}

} // verus!
