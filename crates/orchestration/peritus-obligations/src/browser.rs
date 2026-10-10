//! Browser-semantics requirements and standards-implementation evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::EvidenceBinding;
use peritus_types::Sha256Digest;

#[doc = " Implementation class used to observe claimed browser behavior."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BrowserImplementation {
    #[doc = " Standards-compliant browser engine or equivalent standards implementation."]
    StandardsCompliant,
    #[doc = " Hand-written or library text parser without browser semantics."]
    ParserOnly,
    #[doc = " Simulated browser result without a standards implementation."]
    Simulated,
}
#[doc = " Public browser behavior contract and its required oracle."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BrowserRequirement {
    oracle_identity: Sha256Digest,
}
impl BrowserRequirement {
    #[doc = " Creates a requirement for one standards-compliant oracle."]
    #[must_use]
    pub const fn new(oracle_identity: Sha256Digest) -> Self {
        Self { oracle_identity }
    }
    #[doc = " Required browser oracle identity."]
    #[must_use]
    pub const fn oracle_identity(self) -> Sha256Digest {
        self.oracle_identity
    }
}
#[doc = " Candidate-bound observation of browser behavior."]
#[derive(Debug, Eq, PartialEq)]
pub struct BrowserEvidence {
    binding: EvidenceBinding,
    implementation: BrowserImplementation,
    oracle_identity: Option<Sha256Digest>,
    oracle_passed: bool,
}
impl BrowserEvidence {
    #[doc = " Creates one browser-semantics observation."]
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        implementation: BrowserImplementation,
        oracle_identity: Option<Sha256Digest>,
        oracle_passed: bool,
    ) -> Self {
        Self { binding, implementation, oracle_identity, oracle_passed }
    }
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Observed implementation class."]
    #[must_use]
    pub const fn implementation(&self) -> BrowserImplementation {
        self.implementation
    }
    #[doc = " Actual standards oracle, when one ran."]
    #[must_use]
    pub const fn oracle_identity(&self) -> Option<Sha256Digest> {
        self.oracle_identity
    }
    #[doc = " Whether the oracle accepted the observed behavior."]
    #[must_use]
    pub const fn oracle_passed(&self) -> bool {
        self.oracle_passed
    }
    #[doc = " Whether a standards implementation and the exact public oracle passed."]
    #[must_use]
    pub fn satisfies(&self, requirement: BrowserRequirement) -> bool {
        match (self.implementation, self.oracle_identity, self.oracle_passed) {
            (BrowserImplementation::StandardsCompliant, Some(oracle), true) => {
                matches!(
                    crate::order::compare(
                        oracle.as_bytes(),
                        requirement.oracle_identity().as_bytes()
                    ),
                    core::cmp::Ordering::Equal
                )
            }
            _ => false,
        }
    }
}
impl Clone for BrowserEvidence {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            implementation: self.implementation,
            oracle_identity: self.oracle_identity,
            oracle_passed: self.oracle_passed,
        }
    }
}
