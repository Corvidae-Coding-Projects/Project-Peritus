//! External reviewer identity, completion state, and required scope.

use peritus_types::{ActorId, Sha256Digest};

#[doc = " External reviewer identity and independence facts."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewerIdentity {
    actor: ActorId,
    organization: Sha256Digest,
    context: Sha256Digest,
}
impl ReviewerIdentity {
    #[doc = " Creates exact reviewer identity evidence."]
    #[must_use]
    pub const fn new(actor: ActorId, organization: Sha256Digest, context: Sha256Digest) -> Self {
        Self { actor, organization, context }
    }
    #[doc = " Returns the stable reviewer actor."]
    #[must_use]
    pub const fn actor(&self) -> ActorId {
        self.actor
    }
    #[doc = " Returns the external organization identity digest."]
    #[must_use]
    pub const fn organization(&self) -> Sha256Digest {
        self.organization
    }
    #[doc = " Returns the fresh review-context digest."]
    #[must_use]
    pub const fn context(&self) -> Sha256Digest {
        self.context
    }
}
#[doc = " Review completion state supplied by the external review boundary."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewCompletion {
    #[doc = " The agreed review scope was not completed."]
    Incomplete,
    #[doc = " The complete scope was reviewed and a final report was issued."]
    Completed,
}
impl ReviewCompletion {
    #[must_use]
    pub const fn is_completed(self) -> bool {
        matches!(self, Self::Completed)
    }
}
#[doc = " Mandatory independent external-review scope."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewScope {
    #[doc = " Native sandbox and escape-focused testing on every tier-one platform."]
    SandboxEscape,
    #[doc = " Capability, writer, reviewer, fixer, plugin, and MCP authority isolation."]
    AuthorityIsolation,
    #[doc = " Sealed evaluation, profile protection, promotion, and rollback isolation."]
    EvolutionAndPromotion,
    #[doc = " Dependency, artifact, SBOM, provenance, license, and signature integrity."]
    SupplyChain,
    #[doc = " Unsafe-code and trusted-computing-base inventory completeness."]
    UnsafeAndTrustedComputingBase,
}
impl ReviewScope {
    #[doc = " Complete canonical independent-review scope."]
    pub const ALL: [Self; 5] = [
        Self::SandboxEscape,
        Self::AuthorityIsolation,
        Self::EvolutionAndPromotion,
        Self::SupplyChain,
        Self::UnsafeAndTrustedComputingBase,
    ];
}
