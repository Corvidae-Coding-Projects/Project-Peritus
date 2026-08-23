//! Mandatory scalar facts that must never be silently omitted.

use super::builder::Builder;
use super::encoding::{
    authority_tier_name, choice_name, decimal_u64, digest_value, hex, independence_name,
    phase_name, policy_tier_name, risk_name, role_name,
};
use peritus_policy::{ApprovalRequirement, AuthorityInstant};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

fn instant(
    builder: &mut Builder,
    epoch_name: &str,
    tick_name: &str,
    value: AuthorityInstant,
) -> Result<(), crate::ApprovalError> {
    builder.required_field(epoch_name, &decimal_u64(value.epoch().get()))?;
    builder.required_field(tick_name, &decimal_u64(value.tick_millis()))
}

fn revision(
    builder: &mut Builder,
    value: RevisionTuple,
) -> Result<(), crate::ApprovalError> {
    builder.required_field("acceptance-spec-id", &hex(value.acceptance_spec_id().as_bytes()))?;
    builder.required_field("harness-id", &hex(value.harness_id().as_bytes()))?;
    builder.required_field("workspace-id", &hex(value.workspace_id().as_bytes()))?;
    builder.required_field(
        "workspace-generation",
        &decimal_u64(value.workspace_generation().get()),
    )?;
    builder.required_field(
        "workspace-revision",
        &decimal_u64(value.workspace_revision().get()),
    )?;
    builder.required_field("policy-id", &hex(value.policy_id().as_bytes()))?;
    builder.required_field("provider-profile-id", &hex(value.provider_profile_id().as_bytes()))
}

fn requirement(
    builder: &mut Builder,
    value: &ApprovalRequirement,
) -> Result<(), crate::ApprovalError> {
    builder.required_field("requirement-tier", authority_tier_name(value.minimum_tier()))?;
    let mut roles = String::new();
    let role_values = value.approver_roles();
    let mut index = 0;
    while index < role_values.len()
        invariant 0 <= index <= role_values.len(),
        decreases role_values.len() - index,
    {
        if index > 0 {
            roles.push(',');
        }
        roles.push_str(role_name(role_values[index]));
        index += 1;
    }
    builder.required_field("approver-roles", &roles)?;

    let mut independence = String::new();
    let independence_values = value.independence().as_slice();
    let mut index = 0;
    while index < independence_values.len()
        invariant 0 <= index <= independence_values.len(),
        decreases independence_values.len() - index,
    {
        if index > 0 {
            independence.push(',');
        }
        independence.push_str(independence_name(independence_values[index]));
        index += 1;
    }
    builder.required_field("independence", &independence)?;
    instant(
        builder,
        "requirement-not-before-epoch",
        "requirement-not-before-tick",
        value.validity().not_before(),
    )?;
    instant(
        builder,
        "requirement-expires-at-epoch",
        "requirement-expires-at-tick",
        value.validity().expires_at(),
    )
}

fn resolution(
    builder: &mut Builder,
    value: Option<crate::ApprovalResolutionFacts>,
) -> Result<(), crate::ApprovalError> {
    let Some(value) = value else {
        return builder.required_field("resolution-present", "false");
    };
    builder.required_field("resolution-present", "true")?;
    builder.required_field(
        "resolution-decision-digest",
        &digest_value(value.decision_digest().sha256()),
    )?;
    builder.required_field("resolution-command-id", &hex(value.command_id().as_bytes()))?;
    builder.required_field("resolution-choice", choice_name(value.choice()))?;
    builder.required_field(
        "resolution-registry-revision",
        &decimal_u64(value.registry_revision().get()),
    )?;
    builder.required_field(
        "resolution-registry-digest",
        &digest_value(value.registry_digest()),
    )?;
    builder.required_field(
        "resolution-credential-generation",
        &decimal_u64(value.credential_generation().get()),
    )?;
    instant(
        builder,
        "resolution-valid-until-epoch",
        "resolution-valid-until-tick",
        value.valid_until(),
    )?;
    if let crate::ApprovalChoice::Amend(identity) = value.choice() {
        builder.required_field(
            "resolution-amend-base-policy-id",
            &hex(identity.base_policy_id().as_bytes()),
        )?;
        builder.required_field(
            "resolution-amend-successor-policy-id",
            &hex(identity.successor_policy_id().as_bytes()),
        )?;
        builder.required_field("resolution-amend-tier", policy_tier_name(identity.tier()))?;
        builder.required_field(
            "resolution-amendment-digest",
            &digest_value(identity.amendment_digest()),
        )?;
    }
    Ok(())
}

pub(super) fn render(
    builder: &mut Builder,
    aggregate: &crate::ApprovalAggregate,
) -> Result<(), crate::ApprovalError> {
    let request = aggregate.request();
    let scope = request.scope();
    builder.required_field("phase", phase_name(aggregate.phase()))?;
    builder.required_field("request-id", &hex(request.request_id().as_bytes()))?;
    builder.required_field("request-digest", &digest_value(request.digest().sha256()))?;
    builder.required_field("action-id", &hex(request.action_id().as_bytes()))?;
    builder.required_field("action-digest", &digest_value(request.action_digest().sha256()))?;
    builder.required_field("requester", &hex(request.requester().as_bytes()))?;
    builder.required_field("requester-role", role_name(request.requester_role()))?;
    builder.required_field("scope-actor", &hex(scope.actor_id().as_bytes()))?;
    builder.required_field("scope-role", role_name(scope.role()))?;
    builder.required_field("environment-id", &hex(scope.environment_id().as_bytes()))?;
    revision(builder, scope.revision())?;
    instant(
        builder,
        "scope-not-before-epoch",
        "scope-not-before-tick",
        scope.validity().not_before(),
    )?;
    instant(
        builder,
        "scope-expires-at-epoch",
        "scope-expires-at-tick",
        scope.validity().expires_at(),
    )?;
    match scope.use_limit().remaining() {
        None => builder.required_field("scope-use-limit", "unlimited")?,
        Some(remaining) => {
            builder.required_field("scope-use-limit", &decimal_u64(remaining))?;
        }
    }
    requirement(builder, request.requirement())?;
    instant(
        builder,
        "request-not-before-epoch",
        "request-not-before-tick",
        request.validity().not_before(),
    )?;
    instant(
        builder,
        "request-expires-at-epoch",
        "request-expires-at-tick",
        request.validity().expires_at(),
    )?;
    instant(
        builder,
        "evaluated-at-epoch",
        "evaluated-at-tick",
        request.evaluated_at(),
    )?;
    builder.required_field(
        "challenge-epoch",
        &decimal_u64(request.challenge_epoch().get()),
    )?;
    builder.required_field(
        "challenge-tick",
        &decimal_u64(request.challenge_tick_millis()),
    )?;
    builder.required_field(
        "authority-floor-epoch",
        &decimal_u64(request.authority_time().epoch().get()),
    )?;
    builder.required_field(
        "authority-floor-tick",
        &decimal_u64(request.authority_time().greatest_tick_millis()),
    )?;
    builder.required_field(
        "risk-details-digest",
        &digest_value(request.risk_details_digest()),
    )?;
    let mut risks = String::new();
    let risk_values = request.risks().as_slice();
    let mut index = 0;
    while index < risk_values.len()
        invariant 0 <= index <= risk_values.len(),
        decreases risk_values.len() - index,
    {
        if index > 0 {
            risks.push(',');
        }
        risks.push_str(risk_name(risk_values[index]));
        index += 1;
    }
    builder.required_field("risks", &risks)?;
    resolution(builder, aggregate.resolution())
}

} // verus!
