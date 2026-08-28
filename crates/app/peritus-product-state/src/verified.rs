//! Verus refinement of durable bootstrap ordering.

use vstd::prelude::*;

use crate::BootstrapPhase;

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

/// Executable refinement of [`bootstrap_transition`].
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
