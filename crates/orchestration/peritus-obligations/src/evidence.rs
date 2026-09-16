//! Candidate- and ledger-bound typed obligation evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{BrowserEvidence, LifecycleEvidence, PerformanceEvidence, SchemaEvidence};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

mod binding;
pub use binding::EvidenceBinding;

verus! {

/// Generic direct evidence for hard, conditional, alternative, and generated-output clauses.
#[derive(Debug, Eq, PartialEq)]
pub struct DirectEvidence {
    binding: EvidenceBinding,
    satisfied: bool,
}

impl DirectEvidence {
    /// Exact binding supplied with the direct observation.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }
    /// Exact caller-supplied satisfaction fact.
    pub closed spec fn spec_satisfied(&self) -> bool { self.satisfied }

    /// Creates one direct observation.
    #[must_use]
    pub const fn new(binding: EvidenceBinding, satisfied: bool) -> (value: Self)
        ensures value.spec_binding() == binding, value.spec_satisfied() == satisfied,
    {
        Self { binding, satisfied }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Whether the direct observation satisfied the public clause.
    #[must_use]
    pub const fn satisfied(&self) -> (satisfied: bool)
        ensures satisfied == self.spec_satisfied(),
    { self.satisfied }
}

/// Candidate-bound observation of one public external effect.
#[derive(Debug, Eq, PartialEq)]
pub struct ExternalEffectEvidence {
    binding: EvidenceBinding,
    effect_identity: Sha256Digest,
    observed_at_public_boundary: bool,
    completed: bool,
}

impl ExternalEffectEvidence {
    /// Exact binding supplied with the external observation.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding { self.binding }
    /// Exact identity of the observed effect.
    pub closed spec fn spec_effect_identity(&self) -> Sha256Digest { self.effect_identity }
    /// Exact public-boundary observation flag supplied by the caller.
    pub closed spec fn spec_public_boundary(&self) -> bool { self.observed_at_public_boundary }
    /// Exact completion fact supplied by the caller.
    pub closed spec fn spec_completed(&self) -> bool { self.completed }
    /// Exact identity, public boundary and terminal-completion requirements for an effect.
    pub open spec fn spec_satisfies(&self, effect: Sha256Digest) -> bool {
        self.spec_effect_identity().spec_bytes()@ == effect.spec_bytes()@
            && self.spec_public_boundary() && self.spec_completed()
    }

    /// Creates one external-effect observation.
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        effect_identity: Sha256Digest,
        observed_at_public_boundary: bool,
        completed: bool,
    ) -> (value: Self)
        ensures value.spec_binding() == binding,
            value.spec_effect_identity() == effect_identity,
            value.spec_public_boundary() == observed_at_public_boundary,
            value.spec_completed() == completed,
    {
        Self { binding, effect_identity, observed_at_public_boundary, completed }
    }

    /// Complete current-candidate binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    { &self.binding }

    /// Exact requested effect identity.
    #[must_use]
    pub const fn effect_identity(&self) -> (value: Sha256Digest)
        ensures value == self.spec_effect_identity(),
    { self.effect_identity }

    /// Whether the effect was observed outside the internal model.
    #[must_use]
    pub const fn observed_at_public_boundary(&self) -> (value: bool)
        ensures value == self.spec_public_boundary(),
    {
        self.observed_at_public_boundary
    }

    /// Whether the public effect reached its requested terminal.
    #[must_use]
    pub const fn completed(&self) -> (value: bool)
        ensures value == self.spec_completed(),
    { self.completed }

    /// Whether the exact requested effect was observed as complete at its public boundary.
    #[must_use]
    pub fn satisfies(&self, effect: Sha256Digest) -> (satisfied: bool)
        ensures satisfied == self.spec_satisfies(effect),
    {
        matches!(crate::order::compare(self.effect_identity.as_bytes(), effect.as_bytes()), core::cmp::Ordering::Equal)
            && self.observed_at_public_boundary && self.completed
    }
}

/// Closed typed evidence vocabulary.
#[derive(Debug, Eq, PartialEq)]
pub enum RequirementEvidence {
    Direct(DirectEvidence),
    Performance(PerformanceEvidence),
    Lifecycle(LifecycleEvidence),
    Schema(SchemaEvidence),
    Browser(BrowserEvidence),
    ExternalEffect(ExternalEffectEvidence),
}

impl RequirementEvidence {
    /// Complete variant and field equality preserved by evidence cloning.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Direct(left), Self::Direct(right)) =>
                left.spec_binding().spec_same_content(&right.spec_binding())
                    && left.spec_satisfied() == right.spec_satisfied(),
            (Self::Performance(left), Self::Performance(right)) =>
                left.spec_binding().spec_same_content(&right.spec_binding())
                    && left.spec_workload() == right.spec_workload()
                    && left.spec_baseline() == right.spec_baseline()
                    && left.spec_candidate() == right.spec_candidate()
                    && left.spec_repetitions() == right.spec_repetitions()
                    && left.spec_statistic() == right.spec_statistic()
                    && left.spec_noise() == right.spec_noise()
                    && left.spec_threshold() == right.spec_threshold(),
            (Self::Lifecycle(left), Self::Lifecycle(right)) =>
                left.spec_binding().spec_same_content(&right.spec_binding())
                    && left.spec_named_ingress() == right.spec_named_ingress()
                    && left.spec_control_event() == right.spec_control_event()
                    && left.spec_observed_transition() == right.spec_observed_transition()
                    && left.spec_final_state() == right.spec_final_state()
                    && left.spec_observation_kind() == right.spec_observation_kind(),
            (Self::Schema(left), Self::Schema(right)) => left.spec_same_content(right),
            (Self::Browser(left), Self::Browser(right)) =>
                left.spec_binding().spec_same_content(&right.spec_binding())
                    && left.spec_implementation() == right.spec_implementation()
                    && left.spec_oracle_identity() == right.spec_oracle_identity()
                    && left.spec_oracle_passed() == right.spec_oracle_passed(),
            (Self::ExternalEffect(left), Self::ExternalEffect(right)) =>
                left.spec_binding().spec_same_content(&right.spec_binding())
                    && left.spec_effect_identity() == right.spec_effect_identity()
                    && left.spec_public_boundary() == right.spec_public_boundary()
                    && left.spec_completed() == right.spec_completed(),
            _ => false,
        }
    }

    /// Exact binding selected by the actual evidence variant.
    pub closed spec fn spec_binding(&self) -> EvidenceBinding {
        match self {
            Self::Direct(value) => value.spec_binding(),
            Self::Performance(value) => value.spec_binding(),
            Self::Lifecycle(value) => value.spec_binding(),
            Self::Schema(value) => value.spec_binding(),
            Self::Browser(value) => value.spec_binding(),
            Self::ExternalEffect(value) => value.spec_binding(),
        }
    }

    /// Common provenance binding.
    #[must_use]
    pub const fn binding(&self) -> (value: &EvidenceBinding)
        ensures *value == self.spec_binding(),
    {
        match self {
            Self::Direct(value) => value.binding(),
            Self::Performance(value) => value.binding(),
            Self::Lifecycle(value) => value.binding(),
            Self::Schema(value) => value.binding(),
            Self::Browser(value) => value.binding(),
            Self::ExternalEffect(value) => value.binding(),
        }
    }

    /// Stable requirement identity used for canonical ordering.
    #[must_use]
    pub const fn requirement_id(&self) -> (id: RequirementId)
        ensures id == self.spec_binding().spec_requirement_id(),
    { self.binding().requirement_id() }
}

impl Clone for RequirementEvidence {
    fn clone(&self) -> (value: Self)
        ensures self.spec_same_content(&value),
    {
        match self {
            Self::Direct(inner) => Self::Direct(inner.clone()),
            Self::Performance(inner) => Self::Performance(inner.clone()),
            Self::Lifecycle(inner) => Self::Lifecycle(inner.clone()),
            Self::Schema(inner) => Self::Schema(inner.clone()),
            Self::Browser(inner) => Self::Browser(inner.clone()),
            Self::ExternalEffect(inner) => Self::ExternalEffect(inner.clone()),
        }
    }
}

impl Clone for DirectEvidence {
    fn clone(&self) -> (value: Self)
        ensures value.spec_binding().spec_same_content(&self.spec_binding()),
            value.spec_satisfied() == self.spec_satisfied(),
    {
        Self { binding: self.binding.clone(), satisfied: self.satisfied }
    }
}

impl Clone for ExternalEffectEvidence {
    fn clone(&self) -> (value: Self)
        ensures value.spec_binding().spec_same_content(&self.spec_binding()),
            value.spec_effect_identity() == self.spec_effect_identity(),
            value.spec_public_boundary() == self.spec_public_boundary(),
            value.spec_completed() == self.spec_completed(),
    {
        Self { binding: self.binding.clone(), effect_identity: self.effect_identity,
            observed_at_public_boundary: self.observed_at_public_boundary, completed: self.completed }
    }
}

} // verus!
