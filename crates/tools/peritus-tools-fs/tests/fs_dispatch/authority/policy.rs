//! Writer-scoped workspace mutation capability fixture.

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetSnapshot, ChildBudgetRequest,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_types::{Generation, Sha256Digest};

use super::{Ids, journal};

pub const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

pub fn capability_use(ids: &Ids, action_digest: Sha256Digest) -> CapabilityUseTransition {
    let validity = ValidityWindow::new(instant(10), instant(100)).expect("validity");
    let uses = UseLimit::limited(3).expect("use limit");
    let permissions = PermissionSet::new(vec![permission(ids)]).expect("permissions");
    let scope = CapabilityScope::new(
        ids.actor,
        ActorRole::Writer,
        ids.environment,
        permissions,
        ids.revision,
        validity,
        uses,
    );
    let boundary = AuthorityBoundary::new(
        vec![ids.actor],
        vec![ActorRole::Writer],
        vec![ids.environment],
        PermissionSet::new(vec![permission(ids)]).expect("boundary permissions"),
        ids.revision,
        validity,
        uses,
    )
    .expect("boundary");
    let selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision,
    );
    let ceiling = AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(journal::digest(49), selector, validity, uses)],
        Vec::new(),
    )
    .expect("ceiling");
    let operation = OperationDescriptor::new(
        ids.capability.clone(),
        OperationClass::WorkspaceMutation,
        RiskSet::new(vec![RiskClass::ScopedWrite]).expect("risks"),
    )
    .expect("operation");
    let policy = PolicyDefinition::new(
        ids.revision.policy_id(),
        ceiling,
        OperationRegistry::new(vec![operation]).expect("operations"),
        Vec::new(),
    )
    .expect("policy");
    let plan = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(10)),
            instant(10),
        )
        .expect("policy decision")
        .into_parts()
        .0
        .expect("authorization plan");
    plan.issue(journal::command(50), journal::digest(50))
        .into_capability()
        .try_use(
            CapabilityUseRequest::new(
                ids.action,
                action_digest,
                permission(ids),
                ids.actor,
                ActorRole::Writer,
                ids.environment,
                ids.revision,
                instant(20),
            ),
            journal::digest(51),
        )
        .expect("capability use")
}

pub fn budgets(ids: &Ids) -> (BudgetSnapshot, BudgetSnapshot) {
    let ledger = BudgetLedger::new_root(
        ids.root_budget,
        ids.revision,
        BudgetLimits::new(BudgetAmounts::from_units(100, 1_000, 60_000, 10, 5)),
    );
    let ledger = ledger
        .transition(BudgetCommand::AllocateChild(ChildBudgetRequest::new(
            ids.child_budget,
            ids.root_budget,
            ids.revision,
            BudgetLimits::new(BudgetAmounts::from_units(40, 400, 20_000, 4, 2)),
        )))
        .expect("child budget")
        .into_ledger();
    (
        ledger.account(ids.root_budget).expect("root budget"),
        ledger.account(ids.child_budget).expect("child budget"),
    )
}

fn permission(ids: &Ids) -> Permission {
    Permission::new(ids.resource, ids.capability.clone())
}
