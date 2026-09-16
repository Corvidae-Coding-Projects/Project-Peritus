//! Real public lifecycle-ingress requirements and observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::EvidenceBinding;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Whether lifecycle behavior was observed through a public boundary or only simulated internally.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LifecycleObservationKind {
    /// A named public signal, restart, disconnect, crash, or equivalent ingress was exercised.
    PublicIngress,
    /// An internal reducer or fixture simulated the transition.
    InternalSimulation,
}

/// Public lifecycle contract with an exact ingress and terminal state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LifecycleRequirement {
    named_ingress: Sha256Digest,
    control_event: Sha256Digest,
    expected_transition: Sha256Digest,
    final_state: Sha256Digest,
}

impl LifecycleRequirement {
    /// Exact stored named ingress.
    pub closed spec fn spec_named_ingress(&self) -> Sha256Digest { self.named_ingress }
    /// Exact stored control event.
    pub closed spec fn spec_control_event(&self) -> Sha256Digest { self.control_event }
    /// Exact stored expected transition.
    pub closed spec fn spec_expected_transition(&self) -> Sha256Digest { self.expected_transition }
    /// Exact stored final state.
    pub closed spec fn spec_final_state(&self) -> Sha256Digest { self.final_state }

    /// Creates one fully named public lifecycle requirement.
    #[must_use]
    pub const fn new(
        named_ingress: Sha256Digest,
        control_event: Sha256Digest,
        expected_transition: Sha256Digest,
        final_state: Sha256Digest,
    ) -> (value: Self)
        ensures value.spec_named_ingress() == named_ingress,
            value.spec_control_event() == control_event,
            value.spec_expected_transition() == expected_transition,
            value.spec_final_state() == final_state,
    {
        Self { named_ingress, control_event, expected_transition, final_state }
    }

    /// Named public ingress.
    #[must_use]
    pub const fn named_ingress(self) -> (value: Sha256Digest)
        ensures value == self.spec_named_ingress(),
    { self.named_ingress }

    /// Exact control event.
    #[must_use]
    pub const fn control_event(self) -> (value: Sha256Digest)
        ensures value == self.spec_control_event(),
    { self.control_event }

    /// Required process or service transition.
    #[must_use]
    pub const fn expected_transition(self) -> (value: Sha256Digest)
        ensures value == self.spec_expected_transition(),
    { self.expected_transition }

    /// Required final state.
    #[must_use]
    pub const fn final_state(self) -> (value: Sha256Digest)
        ensures value == self.spec_final_state(),
    { self.final_state }
}

/// Candidate-bound lifecycle evidence.
#[derive(Debug, Eq, PartialEq)]
pub struct LifecycleEvidence {
    binding: EvidenceBinding,
    named_ingress: Sha256Digest,
    control_event: Sha256Digest,
    observed_transition: Sha256Digest,
    final_state: Sha256Digest,
    observation_kind: LifecycleObservationKind,
}

impl LifecycleEvidence {
    /// Exact stored binding.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }
    /// Exact stored named ingress.
    pub closed spec fn spec_named_ingress(&self) -> Sha256Digest { self.named_ingress }
    /// Exact stored control event.
    pub closed spec fn spec_control_event(&self) -> Sha256Digest { self.control_event }
    /// Exact stored observed transition.
    pub closed spec fn spec_observed_transition(&self) -> Sha256Digest { self.observed_transition }
    /// Exact stored final state.
    pub closed spec fn spec_final_state(&self) -> Sha256Digest { self.final_state }
    /// Exact stored observation kind.
    pub closed spec fn spec_observation_kind(&self) -> LifecycleObservationKind { self.observation_kind }

    /// Exact public observation kind and all four required lifecycle identities.
    pub open spec fn spec_satisfies(&self, requirement: LifecycleRequirement) -> bool {
        self.spec_observation_kind() == LifecycleObservationKind::PublicIngress
            && self.spec_named_ingress().spec_bytes()@ == requirement.spec_named_ingress().spec_bytes()@
            && self.spec_control_event().spec_bytes()@ == requirement.spec_control_event().spec_bytes()@
            && self.spec_observed_transition().spec_bytes()@ == requirement.spec_expected_transition().spec_bytes()@
            && self.spec_final_state().spec_bytes()@ == requirement.spec_final_state().spec_bytes()@
    }

    /// Creates complete lifecycle evidence.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        named_ingress: Sha256Digest,
        control_event: Sha256Digest,
        observed_transition: Sha256Digest,
        final_state: Sha256Digest,
        observation_kind: LifecycleObservationKind,
    ) -> (value: Self)
        ensures value.spec_binding() == binding,
            value.spec_named_ingress() == named_ingress,
            value.spec_control_event() == control_event,
            value.spec_observed_transition() == observed_transition,
            value.spec_final_state() == final_state,
            value.spec_observation_kind() == observation_kind,
    {
        Self {
            binding,
            named_ingress,
            control_event,
            observed_transition,
            final_state,
            observation_kind,
        }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Named public ingress.
    #[must_use]
    pub const fn named_ingress(&self) -> (value: Sha256Digest)
        ensures value == self.spec_named_ingress(),
    { self.named_ingress }

    /// Applied control event.
    #[must_use]
    pub const fn control_event(&self) -> (value: Sha256Digest)
        ensures value == self.spec_control_event(),
    { self.control_event }

    /// Observed process or service transition.
    #[must_use]
    pub const fn observed_transition(&self) -> (value: Sha256Digest)
        ensures value == self.spec_observed_transition(),
    { self.observed_transition }

    /// Observed final state.
    #[must_use]
    pub const fn final_state(&self) -> (value: Sha256Digest)
        ensures value == self.spec_final_state(),
    { self.final_state }

    /// Observation boundary.
    #[must_use]
    pub const fn observation_kind(&self) -> (value: LifecycleObservationKind)
        ensures value == self.spec_observation_kind(),
    { self.observation_kind }

    /// Whether the real public ingress produced the exact required transition and final state.
    #[must_use]
    pub fn satisfies(&self, requirement: LifecycleRequirement) -> (satisfied: bool)
        ensures satisfied == self.spec_satisfies(requirement),
    {
        matches!(self.observation_kind, LifecycleObservationKind::PublicIngress)
            && matches!(crate::order::compare(self.named_ingress.as_bytes(), requirement.named_ingress().as_bytes()), core::cmp::Ordering::Equal)
            && matches!(crate::order::compare(self.control_event.as_bytes(), requirement.control_event().as_bytes()), core::cmp::Ordering::Equal)
            && matches!(crate::order::compare(self.observed_transition.as_bytes(), requirement.expected_transition().as_bytes()), core::cmp::Ordering::Equal)
            && matches!(crate::order::compare(self.final_state.as_bytes(), requirement.final_state().as_bytes()), core::cmp::Ordering::Equal)
    }
}

impl Clone for LifecycleEvidence {
    fn clone(&self) -> (value: Self)
        ensures value.spec_binding().spec_same_content(&self.spec_binding()),
            value.spec_named_ingress() == self.spec_named_ingress(),
            value.spec_control_event() == self.spec_control_event(),
            value.spec_observed_transition() == self.spec_observed_transition(),
            value.spec_final_state() == self.spec_final_state(),
            value.spec_observation_kind() == self.spec_observation_kind(),
    {
        Self { binding: self.binding.clone(), named_ingress: self.named_ingress,
            control_event: self.control_event, observed_transition: self.observed_transition,
            final_state: self.final_state, observation_kind: self.observation_kind }
    }
}

} // verus!
