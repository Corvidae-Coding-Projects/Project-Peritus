//! Unprivileged exact lease claims.

use crate::{LeaseHolder, LeaseScope};
use peritus_policy::AuthorityInstant;
use peritus_types::{Generation, RevisionNumber};

#[doc = " A snapshot of one exact active lease."]
#[doc = ""]
#[doc = " A claim is deliberately copyable and unprivileged. It cannot authorize an effect and becomes"]
#[doc = " stale whenever the generation or claim version changes."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LeaseClaim {
    pub(crate) scope: LeaseScope,
    pub(crate) holder: LeaseHolder,
    pub(crate) generation: Generation,
    pub(crate) claim_version: RevisionNumber,
    pub(crate) issued_at: AuthorityInstant,
    pub(crate) expires_at: AuthorityInstant,
}
impl LeaseClaim {
    pub(crate) const fn new(
        scope: LeaseScope,
        holder: LeaseHolder,
        generation: Generation,
        claim_version: RevisionNumber,
        issued_at: AuthorityInstant,
        expires_at: AuthorityInstant,
    ) -> Self {
        Self { scope, holder, generation, claim_version, issued_at, expires_at }
    }
    #[doc = " Returns the exact aggregate scope."]
    #[must_use]
    pub const fn scope(self) -> LeaseScope {
        self.scope
    }
    #[doc = " Returns the exact actor/session holder."]
    #[must_use]
    pub const fn holder(self) -> LeaseHolder {
        self.holder
    }
    #[doc = " Returns the fenced generation."]
    #[must_use]
    pub const fn generation(self) -> Generation {
        self.generation
    }
    #[doc = " Returns the renewal/use-sensitive claim version."]
    #[must_use]
    pub const fn claim_version(self) -> RevisionNumber {
        self.claim_version
    }
    #[doc = " Returns the issuance observation."]
    #[must_use]
    pub const fn issued_at(self) -> AuthorityInstant {
        self.issued_at
    }
    #[doc = " Returns the exclusive expiry boundary."]
    #[must_use]
    pub const fn expires_at(self) -> AuthorityInstant {
        self.expires_at
    }
}
