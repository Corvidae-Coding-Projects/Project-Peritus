//! Exact public-output assertions for the independent generated evaluator oracle.

use super::{
    ModelApproval, ModelDecision, ModelDenial, ModelWindow, PermissionSpec, instant, permission,
    use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTier, AuthorizationDenialReason, CapabilityScope, IndependenceRequirement,
    PolicyDecision, RiskClass,
};
use peritus_types::{ActorId, EnvironmentId, RevisionTuple};

const fn denial_reason(reason: ModelDenial) -> AuthorizationDenialReason {
    match reason {
        ModelDenial::PolicyMismatch => AuthorizationDenialReason::PolicyMismatch,
        ModelDenial::OutsideBoundary => AuthorizationDenialReason::OutsideAuthorityBoundary,
        ModelDenial::Operation => AuthorizationDenialReason::UnknownOperation,
        ModelDenial::Immutable => AuthorizationDenialReason::ImmutableDeny,
        ModelDenial::Restriction => AuthorizationDenialReason::RestrictionDeny,
        ModelDenial::IncompleteCoverage => AuthorizationDenialReason::IncompleteCeilingCoverage,
        ModelDenial::ConstraintConflict => AuthorizationDenialReason::EmptyConstraintIntersection,
        ModelDenial::NotYetValid => AuthorizationDenialReason::NotYetValid,
        ModelDenial::Expired => AuthorizationDenialReason::Expired,
        ModelDenial::ApprovalConflict => AuthorizationDenialReason::ApprovalConstraintConflict,
    }
}

fn roles(bits: u16) -> Vec<ActorRole> {
    let variants = [
        ActorRole::Writer,
        ActorRole::Fixer,
        ActorRole::Reviewer,
        ActorRole::Evaluator,
        ActorRole::GateRunner,
        ActorRole::Orchestrator,
        ActorRole::EvolutionAgent,
        ActorRole::HumanAuthority,
        ActorRole::DaemonService,
        ActorRole::ProviderToolWorker,
        ActorRole::Plugin,
    ];
    variants
        .into_iter()
        .enumerate()
        .filter_map(|(index, role)| (bits & (1 << index) != 0).then_some(role))
        .collect()
}

fn independence(bits: u8) -> Vec<IndependenceRequirement> {
    let variants = [
        IndependenceRequirement::NotRequester,
        IndependenceRequirement::NotActionActor,
        IndependenceRequirement::NoProducingAttemptParticipation,
        IndependenceRequirement::NoReviewParticipation,
    ];
    variants
        .into_iter()
        .enumerate()
        .filter_map(|(index, requirement)| (bits & (1 << index) != 0).then_some(requirement))
        .collect()
}

const fn tier(rank: u8) -> AuthorityTier {
    match rank {
        0 => AuthorityTier::Project,
        1 => AuthorityTier::User,
        2 => AuthorityTier::Organization,
        3 => AuthorityTier::System,
        _ => panic!("invalid model authority tier"),
    }
}

#[allow(clippy::too_many_arguments, reason = "the exact generated scope fields are intentional")]
fn assert_scope(
    actual: &CapabilityScope,
    actor: ActorId,
    environment: EnvironmentId,
    revision: RevisionTuple,
    permission_spec: PermissionSpec,
    validity: ModelWindow,
    uses: Option<u64>,
    diagnostic: &str,
) {
    assert_eq!(actual.actor(), actor, "{diagnostic}: actor");
    assert_eq!(actual.role(), ActorRole::Writer, "{diagnostic}: role");
    assert_eq!(actual.environment(), environment, "{diagnostic}: environment");
    assert_eq!(
        actual.permissions().as_slice(),
        &[permission(permission_spec)],
        "{diagnostic}: permissions"
    );
    assert_eq!(actual.revision(), revision, "{diagnostic}: revision");
    assert_eq!(
        actual.validity(),
        window(1, validity.start, validity.end),
        "{diagnostic}: validity"
    );
    assert_eq!(actual.use_limit(), use_limit(uses), "{diagnostic}: use limit");
}

fn assert_time(
    evaluated_at: peritus_policy::AuthorityInstant,
    time_state: &peritus_policy::AuthorityTimeState,
    diagnostic: &str,
) {
    assert_eq!(evaluated_at, instant(1, 50), "{diagnostic}: evaluated at");
    assert_eq!(time_state.epoch().get(), 1, "{diagnostic}: time epoch");
    assert_eq!(time_state.greatest_tick_millis(), 50, "{diagnostic}: time floor");
}

fn assert_requirement(
    actual: &peritus_policy::ApprovalRequirement,
    expected: ModelApproval,
    diagnostic: &str,
) {
    assert_eq!(actual.minimum_tier(), tier(expected.minimum_tier), "{diagnostic}: minimum tier");
    assert_eq!(actual.approver_roles(), roles(expected.role_bits), "{diagnostic}: approver roles");
    assert_eq!(
        actual.independence().as_slice(),
        independence(expected.independence_bits),
        "{diagnostic}: independence"
    );
    assert_eq!(
        actual.validity(),
        window(1, expected.validity.start, expected.validity.end),
        "{diagnostic}: requirement validity"
    );
}

#[allow(clippy::too_many_arguments, reason = "the independent oracle supplies each exact field")]
pub fn assert_generated_decision_exact(
    actual: &PolicyDecision,
    expected: ModelDecision,
    actor: ActorId,
    environment: EnvironmentId,
    revision: RevisionTuple,
    permission_spec: PermissionSpec,
    seed: u64,
    case: usize,
) {
    let diagnostic = format!("seed {seed:#x} case {case}");
    match expected {
        ModelDecision::Denied(reason) => {
            let denial = actual.denial().unwrap_or_else(|| panic!("{diagnostic}: expected denial"));
            assert_eq!(denial.reason(), denial_reason(reason), "{diagnostic}: denial reason");
            assert_scope(
                denial.scope(),
                actor,
                environment,
                revision,
                permission_spec,
                ModelWindow { start: 0, end: 100 },
                Some(9),
                &diagnostic,
            );
            assert_time(denial.evaluated_at(), denial.time_state(), &diagnostic);
        }
        ModelDecision::Authorized { validity, uses } => {
            let plan = actual
                .authorized_plan()
                .unwrap_or_else(|| panic!("{diagnostic}: expected authorization"));
            assert_scope(
                plan.scope(),
                actor,
                environment,
                revision,
                permission_spec,
                validity,
                uses,
                &diagnostic,
            );
            assert_time(plan.evaluated_at(), plan.time_state(), &diagnostic);
        }
        ModelDecision::ApprovalRequired { validity, uses, requirement } => {
            let challenge = actual
                .escalation_challenge()
                .unwrap_or_else(|| panic!("{diagnostic}: expected approval"));
            assert_scope(
                challenge.scope(),
                actor,
                environment,
                revision,
                permission_spec,
                validity,
                uses,
                &diagnostic,
            );
            assert_requirement(challenge.requirement(), requirement, &diagnostic);
            assert_eq!(
                challenge.risks().as_slice(),
                &[RiskClass::Read],
                "{diagnostic}: mandatory risks"
            );
            assert_time(challenge.evaluated_at(), challenge.time_state(), &diagnostic);
        }
    }
}
