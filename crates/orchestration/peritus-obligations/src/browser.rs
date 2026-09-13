//! Browser-semantics requirements and standards-implementation evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::EvidenceBinding;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Implementation class used to observe claimed browser behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BrowserImplementation {
    /// Standards-compliant browser engine or equivalent standards implementation.
    StandardsCompliant,
    /// Hand-written or library text parser without browser semantics.
    ParserOnly,
    /// Simulated browser result without a standards implementation.
    Simulated,
}

/// Public browser behavior contract and its required oracle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BrowserRequirement {
    oracle_identity: Sha256Digest,
}

impl BrowserRequirement {
    /// Creates a requirement for one standards-compliant oracle.
    #[must_use]
    pub const fn new(oracle_identity: Sha256Digest) -> Self { Self { oracle_identity } }

    /// Required browser oracle identity.
    #[must_use]
    pub const fn oracle_identity(self) -> Sha256Digest { self.oracle_identity }
}

/// Candidate-bound observation of browser behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEvidence {
    binding: EvidenceBinding,
    implementation: BrowserImplementation,
    oracle_identity: Option<Sha256Digest>,
    oracle_passed: bool,
}

impl BrowserEvidence {
    /// Creates one browser-semantics observation.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        implementation: BrowserImplementation,
        oracle_identity: Option<Sha256Digest>,
        oracle_passed: bool,
    ) -> Self {
        Self { binding, implementation, oracle_identity, oracle_passed }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding { &self.binding }

    /// Observed implementation class.
    #[must_use]
    pub const fn implementation(&self) -> BrowserImplementation { self.implementation }

    /// Actual standards oracle, when one ran.
    #[must_use]
    pub const fn oracle_identity(&self) -> Option<Sha256Digest> { self.oracle_identity }

    /// Whether the oracle accepted the observed behavior.
    #[must_use]
    pub const fn oracle_passed(&self) -> bool { self.oracle_passed }

    /// Whether a standards implementation and the exact public oracle passed.
    #[must_use]
    pub fn satisfies(&self, requirement: BrowserRequirement) -> bool {
        self.implementation == BrowserImplementation::StandardsCompliant
            && self.oracle_identity == Some(requirement.oracle_identity())
            && self.oracle_passed
    }
}

} // verus!
