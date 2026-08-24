//! Raw observations returned by sandbox conformance adapters.

use super::{SandboxDecision, SandboxDomain, SandboxFeature, SandboxLifecyclePhase};

/// Complete result of one sandbox behavior fixture.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "lifecycle, confidentiality, containment, and terminal observations are independent"
)]
pub struct SandboxConformanceObservation {
    decision: SandboxDecision,
    lifecycle: SandboxLifecyclePhase,
    denied_domains: Vec<SandboxDomain>,
    resource_observed: u64,
    resource_limit: u64,
    activation_count: u64,
    live_effect_count: u64,
    cancellation_accepted: bool,
    teardown_complete: bool,
    plan_digest: [u8; 32],
    observation_plan_digest: [u8; 32],
    event_sequences: Vec<u64>,
    ordinary_observation_bytes: Vec<u8>,
    process_tree_contained: bool,
    terminal_controlled: bool,
}

impl SandboxConformanceObservation {
    /// Creates one complete sandbox behavior observation.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        clippy::similar_names,
        clippy::too_many_arguments,
        reason = "cross-domain enforcement facts and plan bindings are independent"
    )]
    pub const fn new(
        decision: SandboxDecision,
        lifecycle: SandboxLifecyclePhase,
        denied_domains: Vec<SandboxDomain>,
        resource_observed: u64,
        resource_limit: u64,
        activation_count: u64,
        live_effect_count: u64,
        cancellation_accepted: bool,
        teardown_complete: bool,
        plan_digest: [u8; 32],
        observation_plan_digest: [u8; 32],
        event_sequences: Vec<u64>,
        ordinary_observation_bytes: Vec<u8>,
        process_tree_contained: bool,
        terminal_controlled: bool,
    ) -> Self {
        Self {
            decision,
            lifecycle,
            denied_domains,
            resource_observed,
            resource_limit,
            activation_count,
            live_effect_count,
            cancellation_accepted,
            teardown_complete,
            plan_digest,
            observation_plan_digest,
            event_sequences,
            ordinary_observation_bytes,
            process_tree_contained,
            terminal_controlled,
        }
    }

    /// Returns the stable enforcement decision.
    #[must_use]
    pub const fn decision(&self) -> SandboxDecision {
        self.decision
    }
    /// Returns the final lifecycle phase.
    #[must_use]
    pub const fn lifecycle(&self) -> SandboxLifecyclePhase {
        self.lifecycle
    }
    /// Returns canonical denied domains.
    #[must_use]
    pub fn denied_domains(&self) -> &[SandboxDomain] {
        &self.denied_domains
    }
    /// Returns exactly accounted resource consumption.
    #[must_use]
    pub const fn resource_observed(&self) -> u64 {
        self.resource_observed
    }
    /// Returns the enforced resource ceiling.
    #[must_use]
    pub const fn resource_limit(&self) -> u64 {
        self.resource_limit
    }
    /// Returns backend activation count.
    #[must_use]
    pub const fn activation_count(&self) -> u64 {
        self.activation_count
    }
    /// Returns effects still live after the observation.
    #[must_use]
    pub const fn live_effect_count(&self) -> u64 {
        self.live_effect_count
    }
    /// Returns whether cancellation was accepted.
    #[must_use]
    pub const fn cancellation_accepted(&self) -> bool {
        self.cancellation_accepted
    }
    /// Returns whether backend teardown was complete.
    #[must_use]
    pub const fn teardown_complete(&self) -> bool {
        self.teardown_complete
    }
    /// Returns the checked plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> [u8; 32] {
        self.plan_digest
    }
    /// Returns the plan digest echoed by enforcement observations.
    #[must_use]
    pub const fn observation_plan_digest(&self) -> [u8; 32] {
        self.observation_plan_digest
    }
    /// Returns emitted observation sequences.
    #[must_use]
    pub fn event_sequences(&self) -> &[u64] {
        &self.event_sequences
    }
    /// Returns bounded ordinary observation bytes for secret-canary inspection.
    #[must_use]
    pub fn ordinary_observation_bytes(&self) -> &[u8] {
        &self.ordinary_observation_bytes
    }
    /// Returns whether the complete process tree was contained.
    #[must_use]
    pub const fn process_tree_contained(&self) -> bool {
        self.process_tree_contained
    }
    /// Returns whether PTY and terminal controls were enforced.
    #[must_use]
    pub const fn terminal_controlled(&self) -> bool {
        self.terminal_controlled
    }
}

/// Inert backend preparation and canonicalization observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxPreparationObservation {
    canonical_features: Vec<SandboxFeature>,
    missing_features: Vec<SandboxFeature>,
    plan_digest: [u8; 32],
    preparation_digest: [u8; 32],
    admitted: bool,
    canonical_bytes: Vec<u8>,
    native_effect_count: u64,
}

impl SandboxPreparationObservation {
    /// Creates one complete inert preparation observation.
    #[must_use]
    pub const fn new(
        canonical_features: Vec<SandboxFeature>,
        missing_features: Vec<SandboxFeature>,
        plan_digest: [u8; 32],
        preparation_digest: [u8; 32],
        admitted: bool,
        canonical_bytes: Vec<u8>,
        native_effect_count: u64,
    ) -> Self {
        Self {
            canonical_features,
            missing_features,
            plan_digest,
            preparation_digest,
            admitted,
            canonical_bytes,
            native_effect_count,
        }
    }

    /// Returns sorted unique required features.
    #[must_use]
    pub fn canonical_features(&self) -> &[SandboxFeature] {
        &self.canonical_features
    }
    /// Returns sorted missing backend features.
    #[must_use]
    pub fn missing_features(&self) -> &[SandboxFeature] {
        &self.missing_features
    }
    /// Returns the checked policy-plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> [u8; 32] {
        self.plan_digest
    }
    /// Returns the backend-bound preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> [u8; 32] {
        self.preparation_digest
    }
    /// Returns whether complete backend support was admitted.
    #[must_use]
    pub const fn admitted(&self) -> bool {
        self.admitted
    }
    /// Returns exact canonical plan bytes for determinism and secret-canary inspection.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Returns native effects caused by inert preparation.
    #[must_use]
    pub const fn native_effect_count(&self) -> u64 {
        self.native_effect_count
    }
}
