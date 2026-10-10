//! Real public lifecycle-ingress requirements and observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::EvidenceBinding;
use peritus_types::Sha256Digest;

#[doc = " Whether lifecycle behavior was observed through a public boundary or only simulated internally."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LifecycleObservationKind {
    #[doc = " A named public signal, restart, disconnect, crash, or equivalent ingress was exercised."]
    PublicIngress,
    #[doc = " An internal reducer or fixture simulated the transition."]
    InternalSimulation,
}
#[doc = " Public lifecycle contract with an exact ingress and terminal state."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LifecycleRequirement {
    named_ingress: Sha256Digest,
    control_event: Sha256Digest,
    expected_transition: Sha256Digest,
    final_state: Sha256Digest,
}
impl LifecycleRequirement {
    #[doc = " Creates one fully named public lifecycle requirement."]
    #[must_use]
    pub const fn new(
        named_ingress: Sha256Digest,
        control_event: Sha256Digest,
        expected_transition: Sha256Digest,
        final_state: Sha256Digest,
    ) -> Self {
        Self { named_ingress, control_event, expected_transition, final_state }
    }
    #[doc = " Named public ingress."]
    #[must_use]
    pub const fn named_ingress(self) -> Sha256Digest {
        self.named_ingress
    }
    #[doc = " Exact control event."]
    #[must_use]
    pub const fn control_event(self) -> Sha256Digest {
        self.control_event
    }
    #[doc = " Required process or service transition."]
    #[must_use]
    pub const fn expected_transition(self) -> Sha256Digest {
        self.expected_transition
    }
    #[doc = " Required final state."]
    #[must_use]
    pub const fn final_state(self) -> Sha256Digest {
        self.final_state
    }
}
#[doc = " Candidate-bound lifecycle evidence."]
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
    #[doc = " Creates complete lifecycle evidence."]
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
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Named public ingress."]
    #[must_use]
    pub const fn named_ingress(&self) -> Sha256Digest {
        self.named_ingress
    }
    #[doc = " Applied control event."]
    #[must_use]
    pub const fn control_event(&self) -> Sha256Digest {
        self.control_event
    }
    #[doc = " Observed process or service transition."]
    #[must_use]
    pub const fn observed_transition(&self) -> Sha256Digest {
        self.observed_transition
    }
    #[doc = " Observed final state."]
    #[must_use]
    pub const fn final_state(&self) -> Sha256Digest {
        self.final_state
    }
    #[doc = " Observation boundary."]
    #[must_use]
    pub const fn observation_kind(&self) -> LifecycleObservationKind {
        self.observation_kind
    }
    #[doc = " Whether the real public ingress produced the exact required transition and final state."]
    #[must_use]
    pub fn satisfies(&self, requirement: LifecycleRequirement) -> bool {
        matches!(self.observation_kind, LifecycleObservationKind::PublicIngress)
            && matches!(
                crate::order::compare(
                    self.named_ingress.as_bytes(),
                    requirement.named_ingress().as_bytes()
                ),
                core::cmp::Ordering::Equal
            )
            && matches!(
                crate::order::compare(
                    self.control_event.as_bytes(),
                    requirement.control_event().as_bytes()
                ),
                core::cmp::Ordering::Equal
            )
            && matches!(
                crate::order::compare(
                    self.observed_transition.as_bytes(),
                    requirement.expected_transition().as_bytes()
                ),
                core::cmp::Ordering::Equal
            )
            && matches!(
                crate::order::compare(
                    self.final_state.as_bytes(),
                    requirement.final_state().as_bytes()
                ),
                core::cmp::Ordering::Equal
            )
    }
}
impl Clone for LifecycleEvidence {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            named_ingress: self.named_ingress,
            control_event: self.control_event,
            observed_transition: self.observed_transition,
            final_state: self.final_state,
            observation_kind: self.observation_kind,
        }
    }
}
