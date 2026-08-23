//! Mathematical policy relations refined by executable reducers.

#[cfg(verus_only)]
use crate::{
    ActorRole, AuthorityInstant, AuthorityTimeState, AuthorizationDenialReason,
    AuthorizationRequest, Capability, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, OperationClass, OperationDescriptor,
    Permission, PolicyDecision, PolicyDecisionKind, PolicyDefinition,
    RestrictionLayer, RestrictionRule,
};
#[cfg(verus_only)]
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

/// Exact maximum over mathematical integer bounds.
pub open spec fn maximum_int(left: int, right: int) -> int {
    if left >= right { left } else { right }
}

/// Exact minimum over mathematical integer bounds.
pub open spec fn minimum_int(left: int, right: int) -> int {
    if left <= right { left } else { right }
}

/// Exact intersection of finite and unlimited logical-use bounds.
pub open spec fn minimum_use_limit(left: Option<int>, right: Option<int>) -> Option<int> {
    match (left, right) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(left_value), Some(right_value)) => {
            Some(if left_value <= right_value { left_value } else { right_value })
        }
    }
}

/// Specification relation for non-configurable role separation.
pub open spec fn role_permits(role: ActorRole, operation: OperationClass) -> bool {
    match role {
        ActorRole::Writer | ActorRole::Fixer => matches!(operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
                | OperationClass::RepositoryHistoryMutation
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
        ),
        ActorRole::Reviewer | ActorRole::Plugin => operation == OperationClass::Inspection,
        ActorRole::Evaluator | ActorRole::GateRunner => matches!(operation,
            OperationClass::Inspection | OperationClass::Execution
        ),
        ActorRole::Orchestrator => matches!(operation,
            OperationClass::Inspection
                | OperationClass::Execution
                | OperationClass::Acceptance
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
        ),
        ActorRole::EvolutionAgent => matches!(operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
        ),
        ActorRole::HumanAuthority => matches!(operation,
            OperationClass::Inspection
                | OperationClass::Acceptance
                | OperationClass::Waiver
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
                | OperationClass::HumanAuthority
        ),
        ActorRole::DaemonService => matches!(operation,
            OperationClass::Inspection
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
                | OperationClass::Acceptance
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
        ),
        ActorRole::ProviderToolWorker => matches!(operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
                | OperationClass::RepositoryHistoryMutation
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
                | OperationClass::RawEffect
        ),
    }
}

/// Most-restrictive composition over decision ranks: deny `0`, approval `1`, authorize `2`.
pub open spec fn decision_meet(left: int, right: int) -> int {
    if left <= right { left } else { right }
}

/// Abstract limited-use successor relation.
pub open spec fn decremented_use(previous: int, successor: int) -> bool {
    previous > 0 && successor == previous - 1
}

/// Exact successor relation for unlimited or positive limited use bounds.
pub open spec fn use_limit_successor(
    previous: Option<int>,
    successor: Option<int>,
) -> bool {
    match previous {
        None => successor.is_none(),
        Some(value) => value > 0 && successor == Some(value - 1),
    }
}

/// Exact fixed-width identifier suffix equality from a verified byte index.
pub open spec fn same_identifier_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_identifier_from(left, right, index + 1)
    }
}

/// Exact fixed-width identifier equality through all sixteen bytes.
pub open spec fn same_identifier(left: [u8; 16], right: [u8; 16]) -> bool {
    same_identifier_from(left, right, 0)
}

/// Exact componentwise revision identity used across capability-use refinement.
pub open spec fn same_revision(left: RevisionTuple, right: RevisionTuple) -> bool {
    same_identifier(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    )
        && same_identifier(
            left.spec_harness_id().spec_bytes(),
            right.spec_harness_id().spec_bytes(),
        )
        && same_identifier(
            left.spec_workspace_id().spec_bytes(),
            right.spec_workspace_id().spec_bytes(),
        )
        && left.spec_workspace_generation().spec_value()
            == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && same_identifier(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && same_identifier(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
}

/// Exact successful capability-use refinement over scope, action, time, and consumption.
pub open spec fn capability_use_success(
    prior: &Capability,
    request: &CapabilityUseRequest,
    transition_digest: peritus_types::Sha256Digest,
    transition: &CapabilityUseTransition,
) -> bool {
    same_identifier(request.spec_actor_id(), prior.spec_scope_actor_id())
        && request.spec_role() == prior.spec_scope_role()
        && same_identifier(
            request.spec_environment_id(),
            prior.spec_scope_environment_id(),
        )
        && same_revision(request.spec_revision(), prior.spec_scope_revision())
        && prior.spec_scope_contains_permission(&request.spec_permission())
        && prior.spec_scope_validity().spec_contains(request.spec_observed_at())
        && request.spec_observed_at().spec_epoch() == prior.spec_time_epoch()
        && request.spec_observed_at().spec_tick_millis() >= prior.spec_greatest_tick()
        && transition.spec_action_id() == request.spec_action_id()
        && transition.spec_action_digest() == request.spec_action_digest()
        && transition.spec_permission_resource_id() == request.spec_permission_resource_id()
        && transition.spec_permission_capability_name()
            == request.spec_permission_capability_name()
        && transition.spec_used_at() == request.spec_observed_at()
        && transition.spec_transition_digest() == transition_digest.spec_bytes()
        && transition.spec_scope_actor_id() == prior.spec_scope_actor_id()
        && transition.spec_scope_role() == prior.spec_scope_role()
        && transition.spec_scope_environment_id() == prior.spec_scope_environment_id()
        && transition.spec_scope_permissions() == prior.spec_scope_permissions()
        && transition.spec_scope_revision() == prior.spec_scope_revision()
        && transition.spec_scope_validity() == prior.spec_scope_validity()
        && transition.spec_scope_use_limit() == prior.spec_scope_use_limit()
        && transition.spec_previous_remaining_uses() == prior.spec_remaining_uses()
        && transition.spec_successor_time_epoch()
            == request.spec_observed_at().spec_epoch()
        && transition.spec_successor_greatest_tick()
            == request.spec_observed_at().spec_tick_millis()
        && transition.spec_successor_issued_at() == prior.spec_issued_at()
        && transition.spec_successor_issuance_digest() == prior.spec_issuance_digest()
        && transition.spec_successor_issuance_command_id()
            == prior.spec_issuance_command_id()
        && use_limit_successor(
            prior.spec_remaining_uses(),
            transition.spec_successor_remaining_uses(),
        )
}

/// Concrete safety relation refined by whole-request executable policy evaluation.
pub open spec fn policy_evaluation_safety(
    policy: &PolicyDefinition,
    request: &AuthorizationRequest,
    previous_time: AuthorityTimeState,
    observed_at: AuthorityInstant,
    decision: &PolicyDecision,
) -> bool {
    let scope = request.spec_scope_value();
    let denied = decision.spec_kind() == PolicyDecisionKind::Denied;
    let approvals = crate::approval_fold_model::policy_approval_values(policy, &scope);
    let constrained_approval_conflict = crate::approval_fold_model::effective_approval_conflict(
        approvals,
        decision.spec_scope_validity(),
    );
    (!policy.spec_matches_policy_id(&scope) ==> denied)
        && (!policy.spec_boundary_contains(&scope) ==> denied)
        && (policy.spec_first_operation_denial(&scope).is_some() ==> denied)
        && (policy.spec_has_immutable_deny(&scope) ==> denied)
        && (!policy.spec_has_full_coverage(&scope) ==> denied)
        && (policy.spec_has_restriction_deny(&scope) ==> denied)
        && (!denied ==> policy.spec_matches_policy_id(&scope))
        && (!denied ==> policy.spec_boundary_contains(&scope))
        && (!denied ==> policy.spec_first_operation_denial(&scope).is_none())
        && (!denied ==> !policy.spec_has_immutable_deny(&scope))
        && (!denied ==> policy.spec_has_full_coverage(&scope))
        && (!denied ==> !policy.spec_has_restriction_deny(&scope))
        && (!denied ==> crate::constraint_model::decision_has_exact_constraints(
            policy,
            &scope,
            decision,
        ))
        && ((approvals.conflict
                || (approvals.required && constrained_approval_conflict)) ==> denied)
        && (!denied ==> crate::approval_fold_model::decision_has_exact_approval(
            policy,
            &scope,
            decision,
        ))
        && (!denied ==> crate::monotonicity_model::decision_respects_allowed_query_set(
            policy,
            &scope,
            decision,
        ))
        && decision.spec_evaluated_at() == observed_at
        && decision.spec_time_epoch() == observed_at.spec_epoch()
        && decision.spec_greatest_tick() == observed_at.spec_tick_millis()
        && observed_at.spec_epoch() == previous_time.spec_epoch()
        && observed_at.spec_tick_millis() >= previous_time.spec_greatest_tick_millis()
        && (!denied ==> decision.spec_scope_validity().spec_contains(observed_at))
}

/// Constructive suffix search for one matching ceiling grant.
pub open spec fn grant_covers_permission_from(
    grants: Seq<CeilingGrant>,
    scope: &CapabilityScope,
    permission: &Permission,
    index: nat,
) -> bool
    decreases grants.len() - index,
{
    if index >= grants.len() {
        false
    } else {
        (grants[index as int].spec_matches_identity(scope)
            && grants[index as int].spec_contains_permission(permission))
            || grant_covers_permission_from(grants, scope, permission, index + 1)
    }
}

/// Constructive whole-request coverage over every exact requested permission.
pub open spec fn full_ceiling_coverage_from(
    permissions: Seq<Permission>,
    grants: Seq<CeilingGrant>,
    scope: &CapabilityScope,
    index: nat,
) -> bool
    decreases permissions.len() - index,
{
    if index >= permissions.len() {
        true
    } else {
        grant_covers_permission_from(grants, scope, &permissions[index as int], 0)
            && full_ceiling_coverage_from(permissions, grants, scope, index + 1)
    }
}

/// Constructive suffix search for a matching explicit deny rule.
pub open spec fn deny_rule_matches_from(
    rules: Seq<RestrictionRule>,
    scope: &CapabilityScope,
    index: nat,
) -> bool
    decreases rules.len() - index,
{
    if index >= rules.len() {
        false
    } else {
        (rules[index as int].spec_is_deny()
            && rules[index as int].spec_matches_scope(scope))
            || deny_rule_matches_from(rules, scope, index + 1)
    }
}

/// Constructive suffix search for a matching denial in lower restriction layers.
pub open spec fn restriction_deny_matches_from(
    layers: Seq<RestrictionLayer>,
    scope: &CapabilityScope,
    index: nat,
) -> bool
    decreases layers.len() - index,
{
    if index >= layers.len() {
        false
    } else {
        deny_rule_matches_from(layers[index as int].spec_rules(), scope, 0)
            || restriction_deny_matches_from(layers, scope, index + 1)
    }
}

/// Exact registry lookup and role-separation result for one capability name.
pub open spec fn descriptor_denial_from(
    descriptors: Seq<OperationDescriptor>,
    role: ActorRole,
    permission: &Permission,
    index: nat,
) -> Option<AuthorizationDenialReason>
    decreases descriptors.len() - index,
{
    if index >= descriptors.len() {
        Some(AuthorizationDenialReason::UnknownOperation)
    } else {
        match descriptors[index as int].spec_name_cmp(permission.spec_capability_name()) {
            core::cmp::Ordering::Less => {
                descriptor_denial_from(descriptors, role, permission, index + 1)
            }
            core::cmp::Ordering::Greater => Some(AuthorizationDenialReason::UnknownOperation),
            core::cmp::Ordering::Equal => {
                if role_permits(role, descriptors[index as int].spec_operation_class()) {
                    None
                } else {
                    Some(AuthorizationDenialReason::RoleSeparation)
                }
            }
        }
    }
}

/// Exact first registered-operation or role-separation denial over a complete request.
pub open spec fn operation_denial_from(
    permissions: Seq<Permission>,
    descriptors: Seq<OperationDescriptor>,
    role: ActorRole,
    index: nat,
) -> Option<AuthorizationDenialReason>
    decreases permissions.len() - index,
{
    if index >= permissions.len() {
        None
    } else {
        match descriptor_denial_from(descriptors, role, &permissions[index as int], 0) {
            Some(reason) => Some(reason),
            None => operation_denial_from(permissions, descriptors, role, index + 1),
        }
    }
}

/// Abstract authority-time monotonicity relation.
pub open spec fn time_advances(previous: int, candidate: int) -> bool {
    candidate >= previous
}

} // verus!
