//! Package-local committed authority and writable-workspace fixture.

mod journal;
mod kernel;
mod lease;
mod workspace;

pub use workspace::{
    artifact_store, authorized_patch, exact_request, intent, receipts, workspace_fixture,
};

use std::fs;

use peritus_budget::{
    BudgetAmounts, BudgetCommand, BudgetLedger, BudgetLimits, BudgetRequest, BudgetSnapshot,
    ChildBudgetRequest,
};
use peritus_codec::CodecLimits;
use peritus_journal::{
    AggregateKind, BudgetCommitRequest, CommittedBudgetTransition, CommittedCapabilityUse,
    CommittedKernelTransition, CommittedLeaseTransition, CurrentAuthorityEpoch,
    ExpectedAuthorityEpoch, HeadExpectation, SqliteJournal,
};
use peritus_leases::LeaseHolder;
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityBoundary, AuthorityCeiling, AuthorityInstant,
    AuthorityTimeState, AuthorizationRequest, CapabilityScope, CapabilityUseRequest,
    CapabilityUseTransition, CeilingGrant, EnvironmentSelector, OperationClass,
    OperationDescriptor, OperationRegistry, Permission, PermissionSelector, PermissionSet,
    PolicyDefinition, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit, ValidityWindow,
};
use peritus_protocol::{AcceptanceContractDto, ActionIntentDto};
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, EnvironmentId,
    HarnessId, PolicyId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    RunId, SessionId, Sha256Digest, TurnId, WorkspaceId,
};

pub struct Ids {
    pub workspace: WorkspaceId,
    pub resource: ResourceId,
    pub environment: EnvironmentId,
    pub actor: ActorId,
    pub session: SessionId,
    pub action: ActionId,
    pub capability: CapabilityName,
    pub revision: RevisionTuple,
    project: ProjectId,
    run: RunId,
    attempt: AttemptId,
    turn: TurnId,
    root_budget: BudgetId,
    child_budget: BudgetId,
    tool_budget: BudgetId,
    reservation: BudgetReservationId,
}

impl Ids {
    pub fn new() -> Self {
        let contract =
            contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
        let workspace = WorkspaceId::new([3; 16]).expect("workspace");
        let revision = RevisionTuple::new(
            contract.id(),
            HarnessId::new([4; 16]).expect("harness"),
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([5; 16]).expect("policy"),
            ProviderProfileId::new([6; 16]).expect("provider"),
        );
        Self {
            workspace,
            resource: ResourceId::new([7; 16]).expect("resource"),
            environment: EnvironmentId::new([8; 16]).expect("environment"),
            actor: ActorId::new([9; 16]).expect("actor"),
            session: SessionId::new([10; 16]).expect("session"),
            action: ActionId::new([11; 16]).expect("action"),
            capability: CapabilityName::new("workspace.mutate".to_owned()).expect("capability"),
            revision,
            project: ProjectId::new([12; 16]).expect("project"),
            run: RunId::new([13; 16]).expect("run"),
            attempt: AttemptId::new([14; 16]).expect("attempt"),
            turn: TurnId::new([15; 16]).expect("turn"),
            root_budget: BudgetId::new([16; 16]).expect("root budget"),
            child_budget: BudgetId::new([17; 16]).expect("child budget"),
            tool_budget: BudgetId::new([18; 16]).expect("tool budget"),
            reservation: BudgetReservationId::new([19; 16]).expect("reservation"),
        }
    }

    pub const fn holder(&self) -> LeaseHolder {
        LeaseHolder::new(self.actor, self.session)
    }

    pub fn for_action_revision(&self, action_seed: u8, revision: RevisionNumber) -> Self {
        Self {
            workspace: self.workspace,
            resource: self.resource,
            environment: self.environment,
            actor: self.actor,
            session: self.session,
            action: ActionId::new([action_seed; 16]).expect("action"),
            capability: self.capability.clone(),
            revision: RevisionTuple::new(
                self.revision.acceptance_spec_id(),
                self.revision.harness_id(),
                self.workspace,
                self.revision.workspace_generation(),
                revision,
                self.revision.policy_id(),
                self.revision.provider_profile_id(),
            ),
            project: self.project,
            run: self.run,
            attempt: self.attempt,
            turn: self.turn,
            root_budget: self.root_budget,
            child_budget: self.child_budget,
            tool_budget: self.tool_budget,
            reservation: self.reservation,
        }
    }

    pub fn for_tool_action(&self, action_seed: u8, capability: &str) -> Self {
        let mut ids = self.for_action_revision(action_seed, self.revision.workspace_revision());
        ids.capability = CapabilityName::new(capability.to_owned()).expect("tool capability");
        ids
    }
}

pub struct AuthorityReceipts {
    pub kernel: CommittedKernelTransition,
    pub capability: CommittedCapabilityUse,
    pub budget: CommittedBudgetTransition,
    pub lease: CommittedLeaseTransition,
    pub epoch: CurrentAuthorityEpoch,
    pub observed_at: AuthorityInstant,
}

pub fn commit_authority(
    store: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
) -> AuthorityReceipts {
    let digest = intent.digest(CodecLimits::PRODUCTION).expect("action digest");
    let capability_use = capability_use(ids, digest);
    let kernel = kernel::commit(store, ids, intent, &capability_use);
    let (capability, lease) = lease::commit(store, ids, capability_use);
    let budget = commit_budget(store, ids, digest);
    store
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
        .expect("allocate authority epoch");
    let epoch = store.current_authority_epoch().expect("authority epoch").expect("epoch present");
    AuthorityReceipts { kernel, capability, budget, lease, epoch, observed_at: instant(20) }
}

pub const fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(peritus_types::Generation::first(), tick)
}

fn capability_use(ids: &Ids, digest: Sha256Digest) -> CapabilityUseTransition {
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
                digest,
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

pub fn kernel_budgets(ids: &Ids) -> (BudgetSnapshot, BudgetSnapshot) {
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

fn commit_budget(
    store: &mut SqliteJournal,
    ids: &Ids,
    digest: Sha256Digest,
) -> CommittedBudgetTransition {
    const ACTIVE_MILLIS: u64 = 30_000;
    let ledger = BudgetLedger::new_root(
        ids.tool_budget,
        ids.revision,
        BudgetLimits::new(BudgetAmounts::from_units(10, 10, ACTIVE_MILLIS, 2, 1)),
    );
    let request = BudgetRequest::new(
        ids.reservation,
        ids.tool_budget,
        ids.revision,
        ids.action,
        digest,
        BudgetAmounts::from_units(0, 0, 0, 1, 0),
        BudgetAmounts::from_units(0, 0, ACTIVE_MILLIS, 0, 0),
    );
    let transition = ledger.transition(BudgetCommand::Begin(request)).expect("budget begin");
    let key = journal::aggregate(AggregateKind::Budget, 80);
    store
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

fn permission(ids: &Ids) -> Permission {
    Permission::new(ids.resource, ids.capability.clone())
}

fn contract_dto() -> AcceptanceContractDto {
    let current = std::env::current_dir().expect("test working directory");
    let path = current
        .ancestors()
        .map(|root| root.join("protocol/fixtures/v1/acceptance-contract.bin"))
        .find(|path| path.is_file())
        .expect("checked-in acceptance contract path");
    let bytes = fs::read(path).expect("checked-in acceptance contract");
    peritus_codec::decode_message(&bytes, CodecLimits::PRODUCTION).expect("contract DTO")
}
