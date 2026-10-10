//! Executable Linux binding and teardown predicates refined with Verus.

#[doc = " Compact projection of every identity required by native admission."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each admission binding remains independently testable"
)]
pub struct NativeBindingFacts {
    #[doc = " Required feature bits have no unsupported member."]
    pub features_covered: bool,
    #[doc = " The checked plan identity is exact."]
    pub plan_exact: bool,
    #[doc = " The admitted descriptor identity is exact."]
    pub descriptor_exact: bool,
    #[doc = " The runtime probe identity is exact."]
    pub probe_exact: bool,
    #[doc = " The authorized preparation identity is exact."]
    pub preparation_exact: bool,
}
#[doc = " Checks every authority-sensitive Linux native binding."]
#[must_use]
pub const fn native_binding_complete(facts: NativeBindingFacts) -> bool {
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.probe_exact
        && facts.preparation_exact
}
#[doc = " Reports whether required features are a subset of observed support."]
#[must_use]
pub const fn support_covers(required: u64, supported: u64) -> bool {
    required & !supported == 0
}
#[doc = " Complete projection of backend, proxy, and secret resource ownership."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeardownFacts {
    #[doc = " All Linux-backend resources are absent."]
    pub backend_resources_empty: bool,
    #[doc = " All managed-proxy resources are absent."]
    pub proxy_resources_empty: bool,
    #[doc = " All secret-delivery resources are absent."]
    pub secret_resources_empty: bool,
}
#[doc = " Checks that no backend, proxy, or secret resource remains owned."]
#[must_use]
pub const fn teardown_complete(facts: TeardownFacts) -> bool {
    facts.backend_resources_empty && facts.proxy_resources_empty && facts.secret_resources_empty
}
#[doc = " Compact projection of fail-closed activation effects."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each activation effect remains independently testable"
)]
pub struct ActivationFacts {
    #[doc = " The runtime supports every required feature."]
    pub supported: bool,
    #[doc = " Every plan, descriptor, probe, and preparation binding is exact."]
    pub binding_exact: bool,
    #[doc = " A target process activation effect occurred."]
    pub process_activated: bool,
    #[doc = " A managed-network activation effect occurred."]
    pub network_activated: bool,
    #[doc = " A secret-delivery activation effect occurred."]
    pub secrets_activated: bool,
}
#[doc = " Checks that an unsupported or mismatched preparation retained zero activation effects."]
#[must_use]
pub const fn unsupported_or_mismatched_has_no_effect(facts: ActivationFacts) -> bool {
    (facts.supported && facts.binding_exact)
        || (!facts.process_activated && !facts.network_activated && !facts.secrets_activated)
}

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
