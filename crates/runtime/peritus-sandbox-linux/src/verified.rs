//! Executable Linux binding and teardown predicates refined with Verus.

use vstd::prelude::*;

verus! {

/// Compact projection of every identity required by native admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each admission binding remains independently testable"
)]
pub struct NativeBindingFacts {
    /// Required feature bits have no unsupported member.
    pub features_covered: bool,
    /// The checked plan identity is exact.
    pub plan_exact: bool,
    /// The admitted descriptor identity is exact.
    pub descriptor_exact: bool,
    /// The runtime probe identity is exact.
    pub probe_exact: bool,
    /// The authorized preparation identity is exact.
    pub preparation_exact: bool,
}

/// Mathematical complete native-admission predicate.
pub open spec fn native_binding_complete_spec(facts: NativeBindingFacts) -> bool {
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.probe_exact
        && facts.preparation_exact
}

/// Checks every authority-sensitive Linux native binding.
#[must_use]
pub const fn native_binding_complete(facts: NativeBindingFacts) -> (result: bool)
    ensures result == native_binding_complete_spec(facts),
{
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.probe_exact
        && facts.preparation_exact
}

/// Reports whether required features are a subset of observed support.
#[must_use]
pub const fn support_covers(required: u64, supported: u64) -> (result: bool)
    ensures result == (required & !supported == 0),
{
    required & !supported == 0
}

/// Complete projection of backend, proxy, and secret resource ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeardownFacts {
    /// All Linux-backend resources are absent.
    pub backend_resources_empty: bool,
    /// All managed-proxy resources are absent.
    pub proxy_resources_empty: bool,
    /// All secret-delivery resources are absent.
    pub secret_resources_empty: bool,
}

/// Mathematical complete Linux teardown predicate.
pub open spec fn teardown_complete_spec(facts: TeardownFacts) -> bool {
    facts.backend_resources_empty
        && facts.proxy_resources_empty
        && facts.secret_resources_empty
}

/// Checks that no backend, proxy, or secret resource remains owned.
#[must_use]
pub const fn teardown_complete(facts: TeardownFacts) -> (result: bool)
    ensures result == teardown_complete_spec(facts),
{
    facts.backend_resources_empty
        && facts.proxy_resources_empty
        && facts.secret_resources_empty
}

/// Compact projection of fail-closed activation effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each activation effect remains independently testable"
)]
pub struct ActivationFacts {
    /// The runtime supports every required feature.
    pub supported: bool,
    /// Every plan, descriptor, probe, and preparation binding is exact.
    pub binding_exact: bool,
    /// A target process activation effect occurred.
    pub process_activated: bool,
    /// A managed-network activation effect occurred.
    pub network_activated: bool,
    /// A secret-delivery activation effect occurred.
    pub secrets_activated: bool,
}

/// Mathematical fail-closed unsupported-or-mismatched predicate.
pub open spec fn unsupported_or_mismatched_no_effect_spec(facts: ActivationFacts) -> bool {
    (facts.supported && facts.binding_exact)
        || (!facts.process_activated && !facts.network_activated && !facts.secrets_activated)
}

/// Checks that an unsupported or mismatched preparation retained zero activation effects.
#[must_use]
pub const fn unsupported_or_mismatched_has_no_effect(facts: ActivationFacts) -> (result: bool)
    ensures result == unsupported_or_mismatched_no_effect_spec(facts),
{
    (facts.supported && facts.binding_exact)
        || (!facts.process_activated && !facts.network_activated && !facts.secrets_activated)
}

} // verus!

use crate::NativePhase;

/// Reports whether one observable lifecycle transition is permitted.
#[must_use]
pub const fn lifecycle_transition_allowed(from: NativePhase, to: NativePhase) -> bool {
    matches!(
        (from, to),
        (NativePhase::Prepared, NativePhase::Activated | NativePhase::Released)
            | (NativePhase::Activated, NativePhase::CancelRequested | NativePhase::Terminated,)
            | (NativePhase::CancelRequested, NativePhase::Terminated)
            | (NativePhase::Terminated, NativePhase::Released)
    )
}

/// Reports exact four-way preparation binding.
#[must_use]
pub fn preparation_matches(actual: [&[u8; 32]; 4], expected: [&[u8; 32]; 4]) -> bool {
    actual == expected
}
