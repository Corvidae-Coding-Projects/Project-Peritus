//! Independent public-API oracle for the bounded approval projection.

use peritus_approval::{
    ApprovalAggregate, ApprovalChoice, ApprovalPhase, MAX_RENDERED_APPROVAL_BYTES,
    MAX_RENDERED_FIELD_BYTES, MAX_RENDERED_PARTICIPANTS, MAX_RENDERED_PERMISSIONS,
    RENDER_TRUNCATION_SUFFIX_BYTES, render_approval,
};
use peritus_policy::{ActorRole, AuthorityTier, IndependenceRequirement, PolicyTier, RiskClass};

struct Reference {
    text: String,
    truncated: bool,
}

impl Reference {
    fn new() -> Self {
        Self { text: String::new(), truncated: false }
    }

    fn field(&mut self, name: &str, value: &str) -> Result<bool, String> {
        if !name.bytes().chain(value.bytes()).all(|byte| (0x20..=0x7e).contains(&byte)) {
            return Err("reference received non-printable typed data".into());
        }
        let length = name.len().saturating_add(value.len()).saturating_add(2);
        if length > MAX_RENDERED_FIELD_BYTES {
            return Err("reference field exceeded the public field bound".into());
        }
        if self.text.len().saturating_add(length)
            > MAX_RENDERED_APPROVAL_BYTES - RENDER_TRUNCATION_SUFFIX_BYTES
        {
            self.truncated = true;
            return Ok(false);
        }
        self.text.push_str(name);
        self.text.push('=');
        self.text.push_str(value);
        self.text.push(';');
        Ok(true)
    }

    fn required(&mut self, name: &str, value: &str) -> Result<(), String> {
        if self.field(name, value)? {
            Ok(())
        } else {
            Err(format!("mandatory reference field did not fit: {name}"))
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

const fn role(value: ActorRole) -> &'static str {
    match value {
        ActorRole::Writer => "writer",
        ActorRole::Fixer => "fixer",
        ActorRole::Reviewer => "reviewer",
        ActorRole::Evaluator => "evaluator",
        ActorRole::GateRunner => "gate-runner",
        ActorRole::Orchestrator => "orchestrator",
        ActorRole::EvolutionAgent => "evolution-agent",
        ActorRole::HumanAuthority => "human-authority",
        ActorRole::DaemonService => "daemon-service",
        ActorRole::ProviderToolWorker => "provider-tool-worker",
        ActorRole::Plugin => "plugin",
    }
}

const fn risk(value: RiskClass) -> &'static str {
    match value {
        RiskClass::Read => "read",
        RiskClass::ScopedWrite => "scoped-write",
        RiskClass::Execution => "execution",
        RiskClass::Network => "network",
        RiskClass::DependencyEnvironment => "dependency-environment",
        RiskClass::RepositoryHistoryMutation => "repository-history-mutation",
        RiskClass::SecretUse => "secret-use",
        RiskClass::ExternalSideEffect => "external-side-effect",
        RiskClass::PolicyAuthority => "policy-authority",
        RiskClass::HarnessPromotion => "harness-promotion",
    }
}

const fn authority_tier(value: AuthorityTier) -> &'static str {
    match value {
        AuthorityTier::Project => "project",
        AuthorityTier::User => "user",
        AuthorityTier::Organization => "organization",
        AuthorityTier::System => "system",
    }
}

const fn independence(value: IndependenceRequirement) -> &'static str {
    match value {
        IndependenceRequirement::NotRequester => "not-requester",
        IndependenceRequirement::NotActionActor => "not-action-actor",
        IndependenceRequirement::NoProducingAttemptParticipation => {
            "no-producing-attempt-participation"
        }
        IndependenceRequirement::NoReviewParticipation => "no-review-participation",
    }
}

const fn policy_tier(value: PolicyTier) -> &'static str {
    match value {
        PolicyTier::System => "system",
        PolicyTier::User => "user",
        PolicyTier::Project => "project",
        PolicyTier::Run => "run",
        PolicyTier::Session => "session",
        PolicyTier::RoleHarness => "role-harness",
    }
}

const fn phase(value: ApprovalPhase) -> &'static str {
    match value {
        ApprovalPhase::Pending => "pending",
        ApprovalPhase::ApprovedOnce => "approved-once",
        ApprovalPhase::AmendmentAuthorized => "amendment-authorized",
        ApprovalPhase::Consumed => "consumed",
        ApprovalPhase::Amended => "amended",
        ApprovalPhase::Denied => "denied",
        ApprovalPhase::Expired => "expired",
        ApprovalPhase::Cancelled => "cancelled",
    }
}

fn instant(
    reference: &mut Reference,
    epoch_name: &str,
    tick_name: &str,
    value: peritus_policy::AuthorityInstant,
) -> Result<(), String> {
    reference.required(epoch_name, &value.epoch().get().to_string())?;
    reference.required(tick_name, &value.tick_millis().to_string())
}

fn expected(aggregate: &ApprovalAggregate) -> Result<(String, bool, usize, usize, usize), String> {
    let request = aggregate.request();
    let scope = request.scope();
    let revision = scope.revision();
    let requirement = request.requirement();
    let mut reference = Reference::new();

    reference.required("phase", phase(aggregate.phase()))?;
    reference.required("request-id", &hex(request.request_id().as_bytes()))?;
    reference.required("request-digest", &hex(request.digest().sha256().as_bytes()))?;
    reference.required("action-id", &hex(request.action_id().as_bytes()))?;
    reference.required("action-digest", &hex(request.action_digest().sha256().as_bytes()))?;
    reference.required("requester", &hex(request.requester().as_bytes()))?;
    reference.required("requester-role", role(request.requester_role()))?;
    reference.required("scope-actor", &hex(scope.actor_id().as_bytes()))?;
    reference.required("scope-role", role(scope.role()))?;
    reference.required("environment-id", &hex(scope.environment_id().as_bytes()))?;
    reference.required("acceptance-spec-id", &hex(revision.acceptance_spec_id().as_bytes()))?;
    reference.required("harness-id", &hex(revision.harness_id().as_bytes()))?;
    reference.required("workspace-id", &hex(revision.workspace_id().as_bytes()))?;
    reference
        .required("workspace-generation", &revision.workspace_generation().get().to_string())?;
    reference.required("workspace-revision", &revision.workspace_revision().get().to_string())?;
    reference.required("policy-id", &hex(revision.policy_id().as_bytes()))?;
    reference.required("provider-profile-id", &hex(revision.provider_profile_id().as_bytes()))?;
    instant(
        &mut reference,
        "scope-not-before-epoch",
        "scope-not-before-tick",
        scope.validity().not_before(),
    )?;
    instant(
        &mut reference,
        "scope-expires-at-epoch",
        "scope-expires-at-tick",
        scope.validity().expires_at(),
    )?;
    reference.required(
        "scope-use-limit",
        &scope
            .use_limit()
            .remaining()
            .map_or_else(|| "unlimited".to_owned(), |value| value.to_string()),
    )?;
    reference.required("requirement-tier", authority_tier(requirement.minimum_tier()))?;
    reference.required(
        "approver-roles",
        &requirement
            .approver_roles()
            .iter()
            .map(|value| role(*value))
            .collect::<Vec<_>>()
            .join(","),
    )?;
    reference.required(
        "independence",
        &requirement
            .independence()
            .as_slice()
            .iter()
            .map(|value| independence(*value))
            .collect::<Vec<_>>()
            .join(","),
    )?;
    instant(
        &mut reference,
        "requirement-not-before-epoch",
        "requirement-not-before-tick",
        requirement.validity().not_before(),
    )?;
    instant(
        &mut reference,
        "requirement-expires-at-epoch",
        "requirement-expires-at-tick",
        requirement.validity().expires_at(),
    )?;
    instant(
        &mut reference,
        "request-not-before-epoch",
        "request-not-before-tick",
        request.validity().not_before(),
    )?;
    instant(
        &mut reference,
        "request-expires-at-epoch",
        "request-expires-at-tick",
        request.validity().expires_at(),
    )?;
    instant(&mut reference, "evaluated-at-epoch", "evaluated-at-tick", request.evaluated_at())?;
    reference.required("challenge-epoch", &request.challenge_epoch().get().to_string())?;
    reference.required("challenge-tick", &request.challenge_tick_millis().to_string())?;
    reference
        .required("authority-floor-epoch", &request.authority_time().epoch().get().to_string())?;
    reference.required(
        "authority-floor-tick",
        &request.authority_time().greatest_tick_millis().to_string(),
    )?;
    reference.required("risk-details-digest", &hex(request.risk_details_digest().as_bytes()))?;
    reference.required(
        "risks",
        &request.risks().as_slice().iter().map(|value| risk(*value)).collect::<Vec<_>>().join(","),
    )?;

    if let Some(resolution) = aggregate.resolution() {
        reference.required("resolution-present", "true")?;
        reference.required(
            "resolution-decision-digest",
            &hex(resolution.decision_digest().sha256().as_bytes()),
        )?;
        reference.required("resolution-command-id", &hex(resolution.command_id().as_bytes()))?;
        let choice = resolution.choice();
        reference.required(
            "resolution-choice",
            match choice {
                ApprovalChoice::Deny => "deny",
                ApprovalChoice::ApproveOnce => "approve-once",
                ApprovalChoice::Amend(_) => "amend",
            },
        )?;
        reference.required(
            "resolution-registry-revision",
            &resolution.registry_revision().get().to_string(),
        )?;
        reference.required(
            "resolution-registry-digest",
            &hex(resolution.registry_digest().as_bytes()),
        )?;
        reference.required(
            "resolution-credential-generation",
            &resolution.credential_generation().get().to_string(),
        )?;
        instant(
            &mut reference,
            "resolution-valid-until-epoch",
            "resolution-valid-until-tick",
            resolution.valid_until(),
        )?;
        if let ApprovalChoice::Amend(identity) = choice {
            reference.required(
                "resolution-amend-base-policy-id",
                &hex(identity.base_policy_id().as_bytes()),
            )?;
            reference.required(
                "resolution-amend-successor-policy-id",
                &hex(identity.successor_policy_id().as_bytes()),
            )?;
            reference.required("resolution-amend-tier", policy_tier(identity.tier()))?;
            reference.required(
                "resolution-amendment-digest",
                &hex(identity.amendment_digest().as_bytes()),
            )?;
        }
    } else {
        reference.required("resolution-present", "false")?;
    }

    let producing = request.producing_participants().as_slice();
    let review = request.review_participants().as_slice();
    let permissions = scope.permissions().as_slice();
    let mut producing_omitted = producing.len().saturating_sub(MAX_RENDERED_PARTICIPANTS);
    let mut review_omitted = review.len().saturating_sub(MAX_RENDERED_PARTICIPANTS);
    let mut permission_omitted = permissions.len().saturating_sub(MAX_RENDERED_PERMISSIONS);
    for value in producing.iter().take(MAX_RENDERED_PARTICIPANTS) {
        producing_omitted +=
            usize::from(!reference.field("producing-participant", &hex(value.as_bytes()))?);
    }
    for value in review.iter().take(MAX_RENDERED_PARTICIPANTS) {
        review_omitted +=
            usize::from(!reference.field("review-participant", &hex(value.as_bytes()))?);
    }
    for value in permissions.iter().take(MAX_RENDERED_PERMISSIONS) {
        let rendered =
            format!("{}:{}", hex(value.resource_id().as_bytes()), value.capability_name().as_str());
        permission_omitted += usize::from(!reference.field("permission", &rendered)?);
    }
    reference.truncated |= producing_omitted > 0 || review_omitted > 0 || permission_omitted > 0;
    reference.text.push_str(&format!(
        "permission-count={}/{permission_omitted};producing-count={}/{producing_omitted};review-count={}/{review_omitted};truncated={};",
        permissions.len(),
        producing.len(),
        review.len(),
        reference.truncated,
    ));
    Ok((reference.text, reference.truncated, permission_omitted, producing_omitted, review_omitted))
}

/// Compares the entire public projection with an independently assembled reference.
pub fn check(aggregate: &ApprovalAggregate) -> Result<(), String> {
    let (expected, truncated, permissions, producing, review) = expected(aggregate)?;
    let actual =
        render_approval(aggregate).map_err(|error| format!("approval render failed: {error:?}"))?;
    if actual.as_str() != expected {
        return Err(format!(
            "approval render differs from public reference\nexpected: {expected}\nactual:   {}",
            actual.as_str()
        ));
    }
    if actual.was_truncated() != truncated
        || actual.omitted_permissions() != permissions
        || actual.omitted_producing_participants() != producing
        || actual.omitted_review_participants() != review
    {
        return Err("approval render metadata differs from the public reference".into());
    }
    if actual.as_str().len() > MAX_RENDERED_APPROVAL_BYTES
        || !actual.as_str().bytes().all(|byte| (0x20..=0x7e).contains(&byte))
    {
        return Err("approval render violated its public size or ASCII contract".into());
    }
    Ok(())
}
