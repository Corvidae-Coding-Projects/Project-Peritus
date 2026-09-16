//! External reviewer identity, completion state, and required scope.

use peritus_types::{ActorId, Sha256Digest};
use vstd::prelude::*;

verus! {

/// External reviewer identity and independence facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewerIdentity {
    actor: ActorId,
    organization: Sha256Digest,
    context: Sha256Digest,
}

impl ReviewerIdentity {
    /// Creates exact reviewer identity evidence.
    #[must_use]
    pub const fn new(
        actor: ActorId,
        organization: Sha256Digest,
        context: Sha256Digest,
    ) -> Self {
        Self { actor, organization, context }
    }

    /// Returns the stable reviewer actor.
    #[must_use]
    pub const fn actor(&self) -> (actor: ActorId)
        ensures actor == self.spec_actor()
    {
        self.actor
    }

    /// Returns the external organization identity digest.
    #[must_use]
    pub const fn organization(&self) -> (organization: Sha256Digest)
        ensures organization == self.spec_organization()
    {
        self.organization
    }

    /// Returns the fresh review-context digest.
    #[must_use]
    pub const fn context(&self) -> (context: Sha256Digest)
        ensures context == self.spec_context()
    {
        self.context
    }

    pub closed spec fn spec_actor(&self) -> ActorId { self.actor }

    pub closed spec fn spec_organization(&self) -> Sha256Digest { self.organization }

    pub closed spec fn spec_context(&self) -> Sha256Digest { self.context }
}

/// Review completion state supplied by the external review boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewCompletion {
    /// The agreed review scope was not completed.
    Incomplete,
    /// The complete scope was reviewed and a final report was issued.
    Completed,
}

impl ReviewCompletion {
    #[must_use]
    pub const fn is_completed(self) -> (completed: bool)
        ensures completed == self.spec_is_completed()
    {
        matches!(self, Self::Completed)
    }

    pub open spec fn spec_is_completed(self) -> bool { self == Self::Completed }
}

/// Mandatory independent external-review scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewScope {
    /// Native sandbox and escape-focused testing on every tier-one platform.
    SandboxEscape,
    /// Capability, writer, reviewer, fixer, plugin, and MCP authority isolation.
    AuthorityIsolation,
    /// Sealed evaluation, profile protection, promotion, and rollback isolation.
    EvolutionAndPromotion,
    /// Dependency, artifact, SBOM, provenance, license, and signature integrity.
    SupplyChain,
    /// Unsafe-code and trusted-computing-base inventory completeness.
    UnsafeAndTrustedComputingBase,
}

impl ReviewScope {
    /// Complete canonical independent-review scope.
    pub const ALL: [Self; 5] = [
        Self::SandboxEscape,
        Self::AuthorityIsolation,
        Self::EvolutionAndPromotion,
        Self::SupplyChain,
        Self::UnsafeAndTrustedComputingBase,
    ];
}

} // verus!
