//! Exact capability and budget commitments for authorized tool dispatch.

use peritus_budget::{BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest};
use peritus_codec::CodecLimits;
use peritus_journal::{
    AggregateKind, BudgetCommitRequest, CapabilityCommitRequest, CommittedBudgetTransition,
    CommittedCapabilityUse, CommittedKernelTransition, CurrentAuthorityEpoch,
    ExpectedAuthorityEpoch, HeadExpectation, SqliteJournal,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_protocol::ActionIntentDto;
use peritus_types::{Generation, Sha256Digest};

use super::{Ids, journal};

pub struct AuthorityReceipts {
    pub kernel: CommittedKernelTransition,
    pub capability: CommittedCapabilityUse,
    pub budget: CommittedBudgetTransition,
    pub epoch: CurrentAuthorityEpoch,
    pub observed_at: AuthorityInstant,
}

pub fn commit_authority(
    journal_store: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
    wall_millis: u64,
    dispatch: bool,
) -> AuthorityReceipts {
    let action_digest = intent.digest(CodecLimits::PRODUCTION).expect("action digest");
    let capability_use = capability_use(ids, action_digest);
    let kernel = super::kernel::commit(journal_store, ids, intent, &capability_use, dispatch);
    let capability = commit_capability(journal_store, ids, capability_use);
    let budget = commit_budget(journal_store, ids, action_digest, wall_millis);
    journal_store
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
        .expect("allocate authority epoch");
    let epoch = journal_store
        .current_authority_epoch()
        .expect("observe authority epoch")
        .expect("authority epoch present");
    AuthorityReceipts { kernel, capability, budget, epoch, observed_at: instant(20) }
}

const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn capability_use(ids: &Ids, action_digest: Sha256Digest) -> CapabilityUseTransition {
    let validity = ValidityWindow::new(instant(10), instant(100)).expect("validity");
    let use_limit = UseLimit::limited(3).expect("use limit");
    let permissions = PermissionSet::new(vec![permission(ids)]).expect("permissions");
    let scope = CapabilityScope::new(
        ids.actor,
        ActorRole::ProviderToolWorker,
        ids.environment,
        permissions,
        ids.revision,
        validity,
        use_limit,
    );
    let boundary = AuthorityBoundary::new(
        vec![ids.actor],
        vec![ActorRole::ProviderToolWorker],
        vec![ids.environment],
        PermissionSet::new(vec![permission(ids)]).expect("boundary permissions"),
        ids.revision,
        validity,
        use_limit,
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
        vec![CeilingGrant::new(journal::digest(49), selector, validity, use_limit)],
        Vec::new(),
    )
    .expect("ceiling");
    let operation = OperationDescriptor::new(
        ids.capability.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).expect("risk set"),
    )
    .expect("operation");
    let policy = PolicyDefinition::new(
        ids.revision.policy_id(),
        ceiling,
        OperationRegistry::new(vec![operation]).expect("operations"),
        Vec::new(),
    )
    .expect("policy");
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(instant(10)),
            instant(10),
        )
        .expect("policy decision");
    let capability = decision
        .into_parts()
        .0
        .expect("authorization plan")
        .issue(journal::command(50), journal::digest(50))
        .into_capability();
    capability
        .try_use(
            CapabilityUseRequest::new(
                ids.action,
                action_digest,
                permission(ids),
                ids.actor,
                ActorRole::ProviderToolWorker,
                ids.environment,
                ids.revision,
                instant(20),
            ),
            journal::digest(51),
        )
        .expect("capability use")
}

fn permission(ids: &Ids) -> Permission {
    Permission::new(ids.resource, ids.capability.clone())
}

fn commit_capability(
    journal_store: &mut SqliteJournal,
    ids: &Ids,
    transition: CapabilityUseTransition,
) -> CommittedCapabilityUse {
    let key = journal::aggregate(AggregateKind::Approval, 70);
    journal_store
        .commit_capability_use(
            CapabilityCommitRequest::new(
                journal::append(
                    key,
                    journal::command(43),
                    1,
                    journal::event(43),
                    None,
                    HeadExpectation::Absent(key),
                    ids.revision,
                ),
                transition,
                None,
            )
            .expect("bind capability"),
        )
        .expect("commit capability")
}

fn commit_budget(
    journal_store: &mut SqliteJournal,
    ids: &Ids,
    action_digest: Sha256Digest,
    wall_millis: u64,
) -> CommittedBudgetTransition {
    let limits = BudgetLimits::new(BudgetAmounts::from_units(10, 10, wall_millis + 1_000, 2, 1));
    let ledger = BudgetLedger::new_root(ids.tool_budget, ids.revision, limits);
    let request = BudgetRequest::new(
        ids.reservation,
        ids.tool_budget,
        ids.revision,
        ids.action,
        action_digest,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(0, 0, wall_millis, 0, 0),
    );
    let transition = ledger.transition(BudgetCommand::Begin(request)).expect("budget begin");
    let key = journal::aggregate(AggregateKind::Budget, 80);
    journal_store
        .commit_budget_transition(
            BudgetCommitRequest::new(
                journal::append(
                    key,
                    journal::command(80),
                    1,
                    journal::event(80),
                    None,
                    HeadExpectation::Absent(key),
                    ids.revision,
                ),
                transition,
                None,
                None,
            )
            .expect("bind budget"),
        )
        .expect("commit budget")
}
