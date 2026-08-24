//! Executable Windows binding, lifecycle, and teardown predicates refined in Verus.

use vstd::prelude::*;

verus! {

/// Independent facts required before a Windows helper manifest can be prepared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "authority-sensitive binding dimensions remain independently testable"
)]
pub struct NativeBindingFacts {
    /// Every required C2 feature is probe-supported.
    pub features_covered: bool,
    /// Execution and checked sandbox plan digests match.
    pub plan_exact: bool,
    /// Descriptor digest matches admission.
    pub descriptor_exact: bool,
    /// Support digest matches admission.
    pub support_exact: bool,
    /// Preparation digest is exact.
    pub preparation_exact: bool,
    /// Helper bytes match the probed identity.
    pub helper_exact: bool,
    /// Workspace resolution matches authorization.
    pub workspace_exact: bool,
    /// AppContainer/restricted-token identity matches the probe.
    pub token_exact: bool,
    /// ACL plan binds the exact normalized path policy.
    pub acl_exact: bool,
    /// Network policy binds the exact target identity and managed proxy route.
    pub network_exact: bool,
    /// Protected handle set matches network/secret requirements.
    pub handles_exact: bool,
}

/// Mathematical complete native-preparation predicate.
pub open spec fn native_binding_complete_spec(facts: NativeBindingFacts) -> bool {
    facts.features_covered
        && facts.plan_exact
        && facts.descriptor_exact
        && facts.support_exact
        && facts.preparation_exact
        && facts.helper_exact
        && facts.workspace_exact
        && facts.token_exact
        && facts.acl_exact
        && facts.network_exact
        && facts.handles_exact
}

/// Returns whether every Windows native binding comparison succeeded.
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
        && facts.workspace_exact
        && facts.token_exact
        && facts.acl_exact
        && facts.network_exact
        && facts.handles_exact
}

/// Mathematical closed Windows lifecycle relation over stable phase ordinals.
pub open spec fn lifecycle_transition_allowed_spec(current: int, next: int) -> bool {
    (current == 0 && next == 1)
        || (current == 1 && (next == 2 || next == 3))
        || (current == 2 && next == 3)
        || (current == 3 && next == 4)
}

/// Checks one non-repeating Windows lifecycle transition.
#[must_use]
pub const fn lifecycle_transition_allowed(current: u8, next: u8) -> (result: bool)
    ensures result == lifecycle_transition_allowed_spec(current as int, next as int),
{
    (current == 0 && next == 1)
        || (current == 1 && (next == 2 || next == 3))
        || (current == 2 && next == 3)
        || (current == 3 && next == 4)
}

/// Recovery records may remain in phase or advance through the lifecycle relation.
#[must_use]
pub const fn recovery_advance_allowed(current: u8, next: u8) -> (result: bool)
    ensures result == (current == next || lifecycle_transition_allowed_spec(current as int, next as int)),
{
    current == next || lifecycle_transition_allowed(current, next)
}

/// Mathematical complete native teardown predicate.
pub open spec fn teardown_complete_spec(
    job_closed: bool,
    helper_reaped: bool,
    acl_restored: bool,
    secret_files_removed: bool,
    handles_closed: bool,
    support_joined: bool,
) -> bool {
    job_closed
        && helper_reaped
        && acl_restored
        && secret_files_removed
        && handles_closed
        && support_joined
}

/// Checks that no backend-owned Windows resource remains.
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "each cleanup dimension remains independently testable"
)]
pub const fn teardown_complete(
    job_closed: bool,
    helper_reaped: bool,
    acl_restored: bool,
    secret_files_removed: bool,
    handles_closed: bool,
    support_joined: bool,
) -> (result: bool)
    ensures result == teardown_complete_spec(
        job_closed,
        helper_reaped,
        acl_restored,
        secret_files_removed,
        handles_closed,
        support_joined,
    ),
{
    job_closed
        && helper_reaped
        && acl_restored
        && secret_files_removed
        && handles_closed
        && support_joined
}

/// Mathematical fail-closed no-effect rule.
pub open spec fn unsupported_has_no_effect_spec(
    supported: bool,
    prepared: bool,
    activated: bool,
) -> bool {
    supported || (!prepared && !activated)
}

/// Returns whether an unsupported result retained zero preparation/activation effects.
#[must_use]
pub const fn unsupported_has_no_effect(
    supported: bool,
    prepared: bool,
    activated: bool,
) -> (result: bool)
    ensures result == unsupported_has_no_effect_spec(supported, prepared, activated),
{
    supported || (!prepared && !activated)
}

} // verus!
