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
    /// Exact stored oracle identity.
    pub closed spec fn spec_oracle_identity(&self) -> Sha256Digest { self.oracle_identity }

    /// Creates a requirement for one standards-compliant oracle.
    #[must_use]
    pub const fn new(oracle_identity: Sha256Digest) -> (value: Self)
        ensures value.spec_oracle_identity() == oracle_identity,
    { Self { oracle_identity } }

    /// Required browser oracle identity.
    #[must_use]
    pub const fn oracle_identity(self) -> (value: Sha256Digest)
        ensures value == self.spec_oracle_identity(),
    { self.oracle_identity }
}

/// Candidate-bound observation of browser behavior.
#[derive(Debug, Eq, PartialEq)]
pub struct BrowserEvidence {
    binding: EvidenceBinding,
    implementation: BrowserImplementation,
    oracle_identity: Option<Sha256Digest>,
    oracle_passed: bool,
}

impl BrowserEvidence {
    /// Exact stored binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }
    /// Exact stored implementation.
    pub closed spec fn spec_implementation(&self) -> BrowserImplementation { self.implementation }
    /// Exact stored oracle identity.
    pub closed spec fn spec_oracle_identity(&self) -> Option<Sha256Digest> { self.oracle_identity }
    /// Exact stored oracle passed.
    pub closed spec fn spec_oracle_passed(&self) -> bool { self.oracle_passed }

    /// Exact standards implementation, observed oracle identity and supplied acceptance fact.
    pub open spec fn spec_satisfies(&self, requirement: BrowserRequirement) -> bool {
        self.spec_implementation() == BrowserImplementation::StandardsCompliant
            && self.spec_oracle_passed()
            && match self.spec_oracle_identity() {
                Some(oracle) => oracle.spec_bytes()@ == requirement.spec_oracle_identity().spec_bytes()@,
                None => false,
            }
    }

    /// Creates one browser-semantics observation.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        implementation: BrowserImplementation,
        oracle_identity: Option<Sha256Digest>,
        oracle_passed: bool,
    ) -> (value: Self)
        ensures value.spec_binding() == binding,
            value.spec_implementation() == implementation,
            value.spec_oracle_identity() == oracle_identity,
            value.spec_oracle_passed() == oracle_passed,
    {
        Self { binding, implementation, oracle_identity, oracle_passed }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Observed implementation class.
    #[must_use]
    pub const fn implementation(&self) -> (value: BrowserImplementation)
        ensures value == self.spec_implementation(),
    { self.implementation }

    /// Actual standards oracle, when one ran.
    #[must_use]
    pub const fn oracle_identity(&self) -> (value: Option<Sha256Digest>)
        ensures value == self.spec_oracle_identity(),
    { self.oracle_identity }

    /// Whether the oracle accepted the observed behavior.
    #[must_use]
    pub const fn oracle_passed(&self) -> (value: bool)
        ensures value == self.spec_oracle_passed(),
    { self.oracle_passed }

    /// Whether a standards implementation and the exact public oracle passed.
    #[must_use]
    pub fn satisfies(&self, requirement: BrowserRequirement) -> (satisfied: bool)
        ensures satisfied == self.spec_satisfies(requirement),
    {
        match (self.implementation, self.oracle_identity, self.oracle_passed) {
            (BrowserImplementation::StandardsCompliant, Some(oracle), true) => {
                matches!(crate::order::compare(oracle.as_bytes(), requirement.oracle_identity().as_bytes()), core::cmp::Ordering::Equal)
            },
            _ => false,
        }
    }
}

impl Clone for BrowserEvidence {
    fn clone(&self) -> (value: Self)
        ensures value.spec_binding().spec_same_content(&self.spec_binding()),
            value.spec_implementation() == self.spec_implementation(),
            value.spec_oracle_identity() == self.spec_oracle_identity(),
            value.spec_oracle_passed() == self.spec_oracle_passed(),
    {
        Self { binding: self.binding.clone(), implementation: self.implementation,
            oracle_identity: self.oracle_identity, oracle_passed: self.oracle_passed }
    }
}

} // verus!
