//! Verus refinements of durable bootstrap and product-selection policy.

use crate::{BootstrapPhase, WorkspaceTrust};

#[doc = " Closed formal model of the durable G4 bootstrap phases."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapPhaseModel {
    #[doc = " Stable non-secret identities are durable."]
    IdentityReady,
    #[doc = " The public approval registry is durable."]
    RegistryReady,
    #[doc = " Strict daemon configuration is durable."]
    ConfigurationReady,
}
#[doc = " Executable refinement of the `bootstrap_transition` specification."]
#[must_use]
pub const fn bootstrap_transition_model_exec(
    current: BootstrapPhaseModel,
    next: BootstrapPhaseModel,
) -> bool {
    matches!(
        (current, next),
        (
            BootstrapPhaseModel::IdentityReady,
            BootstrapPhaseModel::IdentityReady | BootstrapPhaseModel::RegistryReady
        ) | (
            BootstrapPhaseModel::RegistryReady,
            BootstrapPhaseModel::RegistryReady | BootstrapPhaseModel::ConfigurationReady
        ) | (BootstrapPhaseModel::ConfigurationReady, BootstrapPhaseModel::ConfigurationReady)
    )
}

#[doc = " Executable refinement of the provider failover selection invariant."]
#[must_use]
pub const fn provider_failover_shape_model_exec(
    enabled_providers: u64,
    automatic_failover: bool,
) -> bool {
    !automatic_failover || enabled_providers >= 2
}

#[doc = " Closed formal trust model for durable registration-shape decisions."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceTrustModel {
    #[doc = " No executable registration facts may be retained."]
    Restricted,
    #[doc = " All executable registration facts must be retained."]
    Trusted,
}
#[doc = " Executable refinement of the `workspace_registration_shape` specification."]
#[must_use]
pub const fn workspace_registration_shape_model_exec(
    trust: WorkspaceTrustModel,
    registration_fields: u8,
) -> bool {
    match trust {
        WorkspaceTrustModel::Restricted => registration_fields == 0,
        WorkspaceTrustModel::Trusted => registration_fields == 4,
    }
}

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
