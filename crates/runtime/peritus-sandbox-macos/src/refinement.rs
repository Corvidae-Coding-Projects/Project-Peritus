//! Named macOS portions of the C3 formal obligations.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::{NativeBindingFacts, TeardownFacts};

verus! {

/// `OBL-0130-MACOS`: admitted native preparation covers and binds every exact input identity.
pub proof fn admitted_macos_backend_covers_and_binds(facts: NativeBindingFacts)
    requires crate::verified::native_binding_complete_spec(facts),
    ensures
        facts.features_covered,
        facts.plan_exact,
        facts.descriptor_exact,
        facts.support_exact,
        facts.preparation_exact,
        facts.helper_exact,
        facts.manifest_exact,
        facts.profile_exact,
{
}

/// `OBL-0133-MACOS`: complete teardown implies no backend-owned resource remains.
pub proof fn complete_macos_teardown_releases_every_resource(facts: TeardownFacts)
    requires crate::verified::teardown_complete_spec(facts),
    ensures
        facts.helper_quiescent,
        facts.profile_released,
        facts.proxy_released,
        facts.secrets_released,
        facts.support_threads_joined,
{
}

/// `OBL-0134-MACOS`: any unsupported or mismatched preparation has no activation effect.
pub proof fn unsupported_or_mismatched_has_no_activation(
    supported: bool,
    binding_exact: bool,
    manifest_verified: bool,
)
    requires !crate::verified::activation_permitted_spec(
        supported,
        binding_exact,
        manifest_verified,
    ),
    ensures (supported && binding_exact && manifest_verified) == false,
{
}

} // verus!
