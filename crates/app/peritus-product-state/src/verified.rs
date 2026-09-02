//! Verus refinements of durable bootstrap and product-selection policy.

use vstd::prelude::*;

use crate::{BootstrapPhase, WorkspaceTrust};

verus! {

/// Closed formal model of the durable G4 bootstrap phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapPhaseModel {
    /// Stable non-secret identities are durable.
    IdentityReady,
    /// The public approval registry is durable.
    RegistryReady,
    /// Strict daemon configuration is durable.
    ConfigurationReady,
}

/// Exact same-phase or successor relation for resumable bootstrap effects.
pub open spec fn bootstrap_transition(
    current: BootstrapPhaseModel,
    next: BootstrapPhaseModel,
) -> bool {
    matches!(
        (current, next),
        (BootstrapPhaseModel::IdentityReady, BootstrapPhaseModel::IdentityReady | BootstrapPhaseModel::RegistryReady)
            | (BootstrapPhaseModel::RegistryReady, BootstrapPhaseModel::RegistryReady | BootstrapPhaseModel::ConfigurationReady)
            | (BootstrapPhaseModel::ConfigurationReady, BootstrapPhaseModel::ConfigurationReady)
    )
}

/// Executable refinement of the `bootstrap_transition` specification.
#[must_use]
pub const fn bootstrap_transition_model_exec(
    current: BootstrapPhaseModel,
    next: BootstrapPhaseModel,
) -> (result: bool)
    ensures result == bootstrap_transition(current, next)
{
    matches!(
        (current, next),
        (BootstrapPhaseModel::IdentityReady, BootstrapPhaseModel::IdentityReady | BootstrapPhaseModel::RegistryReady)
            | (BootstrapPhaseModel::RegistryReady, BootstrapPhaseModel::RegistryReady | BootstrapPhaseModel::ConfigurationReady)
            | (BootstrapPhaseModel::ConfigurationReady, BootstrapPhaseModel::ConfigurationReady)
    )
}

} // verus!

verus! {

/// A provider switch can be authorized only when another selected route exists.
pub open spec fn provider_failover_shape(
    enabled_providers: u64,
    automatic_failover: bool,
) -> bool {
    !automatic_failover || enabled_providers >= 2
}

/// Executable refinement of the provider failover selection invariant.
#[must_use]
pub const fn provider_failover_shape_model_exec(
    enabled_providers: u64,
    automatic_failover: bool,
) -> (result: bool)
    ensures result == provider_failover_shape(enabled_providers, automatic_failover)
{
    !automatic_failover || enabled_providers >= 2
}

} // verus!

verus! {

/// Closed formal trust model for durable registration-shape decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceTrustModel {
    /// No executable registration facts may be retained.
    Restricted,
    /// All executable registration facts must be retained.
    Trusted,
}

/// Exact number of required registration fields for each trust level.
pub open spec fn workspace_registration_shape(
    trust: WorkspaceTrustModel,
    registration_fields: u8,
) -> bool {
    match trust {
        WorkspaceTrustModel::Restricted => registration_fields == 0,
        WorkspaceTrustModel::Trusted => registration_fields == 4,
    }
}

/// Executable refinement of the `workspace_registration_shape` specification.
#[must_use]
pub const fn workspace_registration_shape_model_exec(
    trust: WorkspaceTrustModel,
    registration_fields: u8,
) -> (result: bool)
    ensures result == workspace_registration_shape(trust, registration_fields)
{
    match trust {
        WorkspaceTrustModel::Restricted => registration_fields == 0,
        WorkspaceTrustModel::Trusted => registration_fields == 4,
    }
}

} // verus!

/// Applies the verified transition predicate to runtime phases.
#[must_use]
pub const fn bootstrap_transition_exec(current: BootstrapPhase, next: BootstrapPhase) -> bool {
    bootstrap_transition_model_exec(model(current), model(next))
}

const fn model(phase: BootstrapPhase) -> BootstrapPhaseModel {
    match phase {
        BootstrapPhase::IdentityReady => BootstrapPhaseModel::IdentityReady,
        BootstrapPhase::RegistryReady => BootstrapPhaseModel::RegistryReady,
        BootstrapPhase::ConfigurationReady => BootstrapPhaseModel::ConfigurationReady,
    }
}

/// Applies the verified registration-shape predicate to runtime workspace trust.
#[must_use]
pub const fn workspace_registration_shape_exec(
    trust: WorkspaceTrust,
    registration_fields: u8,
) -> bool {
    workspace_registration_shape_model_exec(trust_model(trust), registration_fields)
}

const fn trust_model(trust: WorkspaceTrust) -> WorkspaceTrustModel {
    match trust {
        WorkspaceTrust::Restricted => WorkspaceTrustModel::Restricted,
        WorkspaceTrust::Trusted => WorkspaceTrustModel::Trusted,
    }
}

/// Applies the verified failover-selection predicate to a runtime provider count.
#[must_use]
pub const fn provider_failover_shape_exec(
    enabled_providers: u64,
    automatic_failover: bool,
) -> bool {
    provider_failover_shape_model_exec(enabled_providers, automatic_failover)
}
