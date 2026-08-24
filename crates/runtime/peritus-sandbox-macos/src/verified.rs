//! Executable scalar predicates shared with Verus refinement proofs.

use vstd::prelude::*;

verus! {

/// Complete facts required before one native preparation may be returned to C2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independent preparation binding is a separate proof fact"
)]
pub struct NativeBindingFacts {
    /// Required feature bits have no unsupported member.
    pub features_covered: bool,
    /// Execution and checked sandbox plan identities match.
    pub plan_exact: bool,
    /// Probed and admitted descriptors match.
    pub descriptor_exact: bool,
    /// Support digests match.
    pub support_exact: bool,
    /// Preparation digests match.
    pub preparation_exact: bool,
    /// Helper bytes match the probed helper identity.
    pub helper_exact: bool,
    /// Manifest bytes match their checksum and digest.
    pub manifest_exact: bool,
    /// Profile bytes match the manifest profile digest.
    pub profile_exact: bool,
}

/// Mathematical native-binding completeness definition.
pub open spec fn native_binding_complete_spec(facts: NativeBindingFacts) -> bool {
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.support_exact
        && facts.preparation_exact
        && facts.helper_exact
        && facts.manifest_exact
        && facts.profile_exact
}

/// Checks every preparation binding without omitting one authority-relevant identity.
#[must_use]
pub const fn native_binding_complete(facts: NativeBindingFacts) -> (result: bool)
    ensures result == native_binding_complete_spec(facts),
{
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.support_exact
        && facts.preparation_exact
        && facts.helper_exact
        && facts.manifest_exact
        && facts.profile_exact
}

/// Deny-dominant default-deny policy projection.
#[must_use]
pub const fn deny_dominant(allow_matches: bool, deny_matches: bool) -> (allowed: bool)
    ensures allowed == (allow_matches && !deny_matches),
{
    allow_matches && !deny_matches
}

/// Complete teardown facts for every macOS-owned resource family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independent cleanup family is a separate proof fact"
)]
pub struct TeardownFacts {
    /// The complete process group is quiescent.
    pub helper_quiescent: bool,
    /// Profile/session state is released.
    pub profile_released: bool,
    /// Proxy routing lease is released or absent.
    pub proxy_released: bool,
    /// Secret delivery handles are released or absent.
    pub secrets_released: bool,
    /// Every support thread was joined.
    pub support_threads_joined: bool,
}

/// Mathematical complete-teardown definition.
pub open spec fn teardown_complete_spec(facts: TeardownFacts) -> bool {
    facts.helper_quiescent
        && facts.profile_released
        && facts.proxy_released
        && facts.secrets_released
        && facts.support_threads_joined
}

/// Checks that no owned resource remains.
#[must_use]
pub const fn teardown_complete(facts: TeardownFacts) -> (result: bool)
    ensures result == teardown_complete_spec(facts),
{
    facts.helper_quiescent
        && facts.profile_released
        && facts.proxy_released
        && facts.secrets_released
        && facts.support_threads_joined
}

/// Mathematical fail-closed activation predicate.
pub open spec fn activation_permitted_spec(
    supported: bool,
    binding_exact: bool,
    manifest_verified: bool,
) -> bool {
    supported && binding_exact && manifest_verified
}

/// Fail-closed unsupported/mismatch predicate used before activation.
#[must_use]
pub const fn activation_permitted(
    supported: bool,
    binding_exact: bool,
    manifest_verified: bool,
) -> (result: bool)
    ensures result == activation_permitted_spec(supported, binding_exact, manifest_verified),
{
    supported && binding_exact && manifest_verified
}

} // verus!
