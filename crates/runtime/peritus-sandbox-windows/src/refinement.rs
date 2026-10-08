//! Named Windows refinement obligations for C3.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::verified::NativeBindingFacts;

verus! {

/// `OBL-0130`: an admitted Windows session binds every native identity exactly.
pub proof fn windows_native_admission_binds_exact_session(facts: NativeBindingFacts)
    requires crate::verified::native_binding_complete_spec(facts),
    ensures
        facts.features_covered,
        facts.plan_exact,
        facts.descriptor_exact,
        facts.support_exact,
        facts.preparation_exact,
        facts.helper_exact,
        facts.workspace_exact,
        facts.token_exact,
        facts.acl_exact,
        facts.network_exact,
        facts.handles_exact,
{
}

/// `OBL-0133`: complete Windows teardown leaves no backend-owned resource.
#[allow(
    clippy::too_many_arguments,
    reason = "each independently progressing cleanup dimension remains explicit"
)]
pub proof fn windows_complete_teardown_releases_owned_resources(
    job_closed: bool,
    helper_reaped: bool,
    acl_restored: bool,
    secret_files_removed: bool,
    secret_delivery_released: bool,
    handles_closed: bool,
    proxy_joined: bool,
    network_filter_removed: bool,
)
    requires crate::verified::teardown_complete_spec(
        job_closed,
        helper_reaped,
        acl_restored,
        secret_files_removed,
        secret_delivery_released,
        handles_closed,
        proxy_joined,
        network_filter_removed,
    ),
    ensures
        job_closed,
        helper_reaped,
        acl_restored,
        secret_files_removed,
        secret_delivery_released,
        handles_closed,
        proxy_joined,
        network_filter_removed,
{
}

/// `OBL-0134`: unsupported Windows preparation has no native activation effect.
pub proof fn windows_unsupported_preparation_has_no_effect(
    supported: bool,
    prepared: bool,
    activated: bool,
)
    requires
        !supported,
        crate::verified::unsupported_has_no_effect_spec(supported, prepared, activated),
    ensures !prepared, !activated,
{
}

} // verus!
