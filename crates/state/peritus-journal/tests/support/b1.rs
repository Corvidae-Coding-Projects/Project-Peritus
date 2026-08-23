//! Foundation-style constructors for real B1 logical transitions.

use peritus_approval::{ActionDigest, ApprovalRequest, ParticipantSet};
use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest};
use peritus_leases::{LeaseScope, LeaseTransition, LeaseTransitionOutcome};
use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityInstant, AuthorityTier, AuthorityTimeState, AuthorizationRequest, CapabilityScope,
    CapabilityUseRequest, CeilingGrant, EnvironmentSelector, IndependenceRequirement,
    IndependenceSet, OperationClass, OperationDescriptor, OperationRegistry, Permission,
    PermissionSelector, PermissionSet, PolicyDefinition, PolicyTier, RestrictionLayer,
    RestrictionRule, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{
    ActionId, ActorId, ApprovalRequestId, BudgetId, BudgetReservationId, Generation,
};

use super::{DomainIds, command, digest};

pub const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn window(start: u64, end: u64) -> ValidityWindow {
    ValidityWindow::new(instant(start), instant(end)).expect("valid authority window")
}

fn permission(ids: &DomainIds) -> Permission {
    Permission::new(
        ids.resource,
        peritus_types::CapabilityName::new("workspace.mutate".to_owned()).expect("capability name"),
    )
}

fn permission_set(ids: &DomainIds) -> PermissionSet {
    PermissionSet::new(vec![permission(ids)]).expect("permission set")
}

const fn selector(ids: &DomainIds) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision(),
    )
}

fn operation_registry() -> OperationRegistry {
    OperationRegistry::new(vec![
        OperationDescriptor::new(
            peritus_types::CapabilityName::new("workspace.mutate".to_owned())
                .expect("operation name"),
            OperationClass::WorkspaceMutation,
            RiskSet::new(vec![RiskClass::ScopedWrite]).expect("operation risks"),
        )
        .expect("operation descriptor"),
    ])
    .expect("operation registry")
}

fn authority_ceiling(ids: &DomainIds, actor: ActorId) -> AuthorityCeiling {
    let validity = window(0, 100);
    let limit = UseLimit::limited(3).expect("use limit");
    let boundary = AuthorityBoundary::new(
        vec![actor],
        vec![ActorRole::Writer],
        vec![ids.environment],
        permission_set(ids),
        ids.revision(),
        validity,
        limit,
    )
    .expect("authority boundary");
    AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(digest(1), selector(ids), validity, limit)],
        Vec::new(),
    )
    .expect("authority ceiling")
}

pub fn capability_use(ids: &mut DomainIds) -> peritus_policy::CapabilityUseTransition {
    let actor = ids.next(ActorId::new);
    let action = ids.next(ActionId::new);
    let scope = CapabilityScope::new(
        actor,
        ActorRole::Writer,
        ids.environment,
        permission_set(ids),
        ids.revision(),
        window(0, 100),
        UseLimit::limited(3).expect("scope use limit"),
    );
    let policy = PolicyDefinition::new(
        ids.policy,
        authority_ceiling(ids, actor),
        operation_registry(),
        Vec::new(),
    )
    .expect("policy definition");
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(0)),
            instant(10),
        )
        .expect("policy evaluation");
    let (plan, challenge, denial) = decision.into_parts();
    assert!(challenge.is_none());
    assert!(denial.is_none());
    let capability = plan.expect("issuance plan").issue(command(10), digest(10)).into_capability();
    capability
        .try_use(
            CapabilityUseRequest::new(
                action,
                digest(11),
                permission(ids),
                actor,
                ActorRole::Writer,
                ids.environment,
                ids.revision(),
                instant(20),
            ),
            digest(12),
        )
        .expect("capability use")
}

pub fn approval_request(ids: &mut DomainIds) -> ApprovalRequest {
    let requester = ids.next(ActorId::new);
    let requested_scope = CapabilityScope::new(
        requester,
        ActorRole::Writer,
        ids.environment,
        permission_set(ids),
        ids.revision(),
        window(5, 90),
        UseLimit::limited(1).expect("requested use limit"),
    );
    let requirement = ApprovalRequirement::new(
        AuthorityTier::User,
        vec![ActorRole::HumanAuthority],
        IndependenceSet::new(vec![IndependenceRequirement::NotRequester])
            .expect("independence set"),
        window(10, 80),
    )
    .expect("approval requirement");
    let layer = RestrictionLayer::new(
        PolicyTier::Project,
        vec![RestrictionRule::require_approval(digest(20), selector(ids), requirement)],
    )
    .expect("approval restriction");
    let policy = PolicyDefinition::new(
        ids.policy,
        authority_ceiling(ids, requester),
        operation_registry(),
        vec![layer],
    )
    .expect("approval policy");
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(requested_scope),
            AuthorityTimeState::new(instant(0)),
            instant(20),
        )
        .expect("approval policy evaluation");
    let (plan, challenge, denial) = decision.into_parts();
    assert!(plan.is_none());
    assert!(denial.is_none());
    ApprovalRequest::new(
        ids.next(ApprovalRequestId::new),
        ids.next(ActionId::new),
        ActionDigest::from_sha256(digest(21)),
        requester,
        ActorRole::Writer,
        challenge.expect("approval challenge"),
        digest(22),
        ParticipantSet::producing(Vec::new()).expect("producing participants"),
        ParticipantSet::review(Vec::new()).expect("review participants"),
        window(10, 80),
    )
    .expect("approval request")
}

pub fn accepted_lease(outcome: LeaseTransitionOutcome) -> LeaseTransition {
    match outcome {
        LeaseTransitionOutcome::Accepted(transition) => transition,
        LeaseTransitionOutcome::Rejected(failure) => {
            panic!("lease transition rejected: {:?}", failure.error())
        }
    }
}

pub struct HeldBudgetFixture {
    pub begin: peritus_budget::BudgetTransition,
    pub reservation_id: BudgetReservationId,
    pub action_id: ActionId,
    pub reserve: BudgetAmounts,
}

pub fn held_budget(ids: &mut DomainIds) -> HeldBudgetFixture {
    let budget_id = ids.next(BudgetId::new);
    let reservation_id = ids.next(BudgetReservationId::new);
    let action_id = ids.next(ActionId::new);
    let reserve = BudgetAmounts::from_units(8, 5, 3, 0, 0);
    let ledger = BudgetLedger::new_root(
        budget_id,
        ids.revision(),
        BudgetLimits::new(BudgetAmounts::from_units(20, 10, 6, 1, 0)),
    );
    let request = BudgetRequest::new(
        reservation_id,
        budget_id,
        ids.revision(),
        action_id,
        digest(30),
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        reserve,
    );
    let begin = ledger.transition(BudgetCommand::Begin(request)).expect("begin budget");
    HeldBudgetFixture { begin, reservation_id, action_id, reserve }
}

pub fn lease_key(scope: LeaseScope) -> Vec<u8> {
    let mut key = Vec::with_capacity(48);
    key.extend_from_slice(scope.workspace_id().as_bytes());
    key.extend_from_slice(scope.resource_id().as_bytes());
    key.extend_from_slice(scope.environment_id().as_bytes());
    key
}
