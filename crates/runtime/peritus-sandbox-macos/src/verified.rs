//! Executable scalar predicates shared with Verus refinement proofs.

#[doc = " Complete facts required before one native preparation may be returned to C2."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independent preparation binding is a separate proof fact"
)]
pub struct NativeBindingFacts {
    #[doc = " Required feature bits have no unsupported member."]
    pub features_covered: bool,
    #[doc = " Execution and checked sandbox plan identities match."]
    pub plan_exact: bool,
    #[doc = " Probed and admitted descriptors match."]
    pub descriptor_exact: bool,
    #[doc = " Support digests match."]
    pub support_exact: bool,
    #[doc = " Preparation digests match."]
    pub preparation_exact: bool,
    #[doc = " Helper bytes match the probed helper identity."]
    pub helper_exact: bool,
    #[doc = " Manifest bytes match their checksum and digest."]
    pub manifest_exact: bool,
    #[doc = " Profile bytes match the manifest profile digest."]
    pub profile_exact: bool,
}
#[doc = " Checks every preparation binding without omitting one authority-relevant identity."]
#[must_use]
pub const fn native_binding_complete(facts: NativeBindingFacts) -> bool {
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.support_exact
        && facts.preparation_exact
        && facts.helper_exact
        && facts.manifest_exact
        && facts.profile_exact
}
#[doc = " Deny-dominant default-deny policy projection."]
#[must_use]
pub const fn deny_dominant(allow_matches: bool, deny_matches: bool) -> bool {
    allow_matches && !deny_matches
}
#[doc = " Complete teardown facts for every macOS-owned resource family."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each independent cleanup family is a separate proof fact"
)]
pub struct TeardownFacts {
    #[doc = " The complete process group is quiescent."]
    pub helper_quiescent: bool,
    #[doc = " Profile/session state is released."]
    pub profile_released: bool,
    #[doc = " Proxy routing lease is released or absent."]
    pub proxy_released: bool,
    #[doc = " Secret delivery handles are released or absent."]
    pub secrets_released: bool,
    #[doc = " Every support thread was joined."]
    pub support_threads_joined: bool,
}
#[doc = " Checks that no owned resource remains."]
#[must_use]
pub const fn teardown_complete(facts: TeardownFacts) -> bool {
    facts.helper_quiescent
        && facts.profile_released
        && facts.proxy_released
        && facts.secrets_released
        && facts.support_threads_joined
}
#[doc = " Fail-closed unsupported/mismatch predicate used before activation."]
#[must_use]
pub const fn activation_permitted(
    supported: bool,
    binding_exact: bool,
    manifest_verified: bool,
) -> bool {
    supported && binding_exact && manifest_verified
}
