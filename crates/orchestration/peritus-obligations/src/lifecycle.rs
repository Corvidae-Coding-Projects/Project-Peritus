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
    /// Creates one fully named public lifecycle requirement.
    #[must_use]
    pub const fn new(
        named_ingress: Sha256Digest,
        control_event: Sha256Digest,
        expected_transition: Sha256Digest,
        final_state: Sha256Digest,
    ) -> Self {
        Self { named_ingress, control_event, expected_transition, final_state }
    }

    /// Named public ingress.
    #[must_use]
    pub const fn named_ingress(self) -> Sha256Digest { self.named_ingress }

    /// Exact control event.
    #[must_use]
    pub const fn control_event(self) -> Sha256Digest { self.control_event }

    /// Required process or service transition.
    #[must_use]
    pub const fn expected_transition(self) -> Sha256Digest { self.expected_transition }

    /// Required final state.
    #[must_use]
    pub const fn final_state(self) -> Sha256Digest { self.final_state }
}

/// Candidate-bound lifecycle evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LifecycleEvidence {
    binding: EvidenceBinding,
    named_ingress: Sha256Digest,
    control_event: Sha256Digest,
    observed_transition: Sha256Digest,
    final_state: Sha256Digest,
    observation_kind: LifecycleObservationKind,
}

impl LifecycleEvidence {
    /// Creates complete lifecycle evidence.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        named_ingress: Sha256Digest,
        control_event: Sha256Digest,
        observed_transition: Sha256Digest,
        final_state: Sha256Digest,
        observation_kind: LifecycleObservationKind,
    ) -> Self {
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
    pub const fn binding(&self) -> &EvidenceBinding { &self.binding }

    /// Named public ingress.
    #[must_use]
    pub const fn named_ingress(&self) -> Sha256Digest { self.named_ingress }

    /// Applied control event.
    #[must_use]
    pub const fn control_event(&self) -> Sha256Digest { self.control_event }

    /// Observed process or service transition.
    #[must_use]
    pub const fn observed_transition(&self) -> Sha256Digest { self.observed_transition }

    /// Observed final state.
    #[must_use]
    pub const fn final_state(&self) -> Sha256Digest { self.final_state }

    /// Observation boundary.
    #[must_use]
    pub const fn observation_kind(&self) -> LifecycleObservationKind { self.observation_kind }

    /// Whether the real public ingress produced the exact required transition and final state.
    #[must_use]
    pub fn satisfies(&self, requirement: LifecycleRequirement) -> bool {
        self.observation_kind == LifecycleObservationKind::PublicIngress
            && self.named_ingress == requirement.named_ingress()
            && self.control_event == requirement.control_event()
            && self.observed_transition == requirement.expected_transition()
            && self.final_state == requirement.final_state()
    }
}

} // verus!
