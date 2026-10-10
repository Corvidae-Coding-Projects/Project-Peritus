//! Candidate- and ledger-bound typed obligation evidence.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{BrowserEvidence, LifecycleEvidence, PerformanceEvidence, SchemaEvidence};
use peritus_spec::RequirementId;
use peritus_types::Sha256Digest;

mod binding;
pub use binding::EvidenceBinding;

#[doc = " Generic direct evidence for hard, conditional, alternative, and generated-output clauses."]
#[derive(Debug, Eq, PartialEq)]
pub struct DirectEvidence {
    binding: EvidenceBinding,
    satisfied: bool,
}
impl DirectEvidence {
    #[doc = " Creates one direct observation."]
    #[must_use]
    pub const fn new(binding: EvidenceBinding, satisfied: bool) -> Self {
        Self { binding, satisfied }
    }
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Whether the direct observation satisfied the public clause."]
    #[must_use]
    pub const fn satisfied(&self) -> bool {
        self.satisfied
    }
}
#[doc = " Candidate-bound observation of one public external effect."]
#[derive(Debug, Eq, PartialEq)]
pub struct ExternalEffectEvidence {
    binding: EvidenceBinding,
    effect_identity: Sha256Digest,
    observed_at_public_boundary: bool,
    completed: bool,
}
impl ExternalEffectEvidence {
    #[doc = " Creates one external-effect observation."]
    #[must_use]
    pub const fn new(
        binding: EvidenceBinding,
        effect_identity: Sha256Digest,
        observed_at_public_boundary: bool,
        completed: bool,
    ) -> Self {
        Self { binding, effect_identity, observed_at_public_boundary, completed }
    }
    #[doc = " Complete current-candidate binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        &self.binding
    }
    #[doc = " Exact requested effect identity."]
    #[must_use]
    pub const fn effect_identity(&self) -> Sha256Digest {
        self.effect_identity
    }
    #[doc = " Whether the effect was observed outside the internal model."]
    #[must_use]
    pub const fn observed_at_public_boundary(&self) -> bool {
        self.observed_at_public_boundary
    }
    #[doc = " Whether the public effect reached its requested terminal."]
    #[must_use]
    pub const fn completed(&self) -> bool {
        self.completed
    }
    #[doc = " Whether the exact requested effect was observed as complete at its public boundary."]
    #[must_use]
    pub fn satisfies(&self, effect: Sha256Digest) -> bool {
        matches!(
            crate::order::compare(self.effect_identity.as_bytes(), effect.as_bytes()),
            core::cmp::Ordering::Equal
        ) && self.observed_at_public_boundary
            && self.completed
    }
}
#[doc = " Closed typed evidence vocabulary."]
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
    #[doc = " Common provenance binding."]
    #[must_use]
    pub const fn binding(&self) -> &EvidenceBinding {
        match self {
            Self::Direct(value) => value.binding(),
            Self::Performance(value) => value.binding(),
            Self::Lifecycle(value) => value.binding(),
            Self::Schema(value) => value.binding(),
            Self::Browser(value) => value.binding(),
            Self::ExternalEffect(value) => value.binding(),
        }
    }
    #[doc = " Stable requirement identity used for canonical ordering."]
    #[must_use]
    pub const fn requirement_id(&self) -> RequirementId {
        self.binding().requirement_id()
    }
}
impl Clone for RequirementEvidence {
    fn clone(&self) -> Self {
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
    fn clone(&self) -> Self {
        Self { binding: self.binding.clone(), satisfied: self.satisfied }
    }
}
impl Clone for ExternalEffectEvidence {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            effect_identity: self.effect_identity,
            observed_at_public_boundary: self.observed_at_public_boundary,
            completed: self.completed,
        }
    }
}
