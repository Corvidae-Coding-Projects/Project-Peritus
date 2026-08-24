//! Package-local committed authority and writable-workspace fixture.

mod budget;
mod journal;
mod kernel;
mod lease;
mod policy;
mod workspace;

pub use workspace::{exact_request, intent, receipts, workspace_fixture};

use std::fs;

use peritus_codec::CodecLimits;
use peritus_journal::{
    CommittedBudgetTransition, CommittedCapabilityUse, CommittedKernelTransition,
    CommittedLeaseTransition, CurrentAuthorityEpoch, ExpectedAuthorityEpoch, SqliteJournal,
};
use peritus_leases::LeaseHolder;
use peritus_protocol::{AcceptanceContractDto, ActionIntentDto};
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, EnvironmentId,
    HarnessId, PolicyId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    RunId, SessionId, TurnId, WorkspaceId,
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
    pub observed_at: peritus_policy::AuthorityInstant,
}

pub fn commit_authority(
    store: &mut SqliteJournal,
    ids: &Ids,
    intent: &ActionIntentDto,
) -> AuthorityReceipts {
    let digest = intent.digest(CodecLimits::PRODUCTION).expect("action digest");
    let capability_use = policy::capability_use(ids, digest);
    let kernel = kernel::commit(store, ids, intent, &capability_use);
    let (capability, lease) = lease::commit(store, ids, capability_use);
    let budget = budget::commit(store, ids, digest);
    store
        .allocate_authority_epoch(ExpectedAuthorityEpoch::Absent)
        .expect("allocate authority epoch");
    let epoch = store.current_authority_epoch().expect("authority epoch").expect("epoch present");
    AuthorityReceipts { kernel, capability, budget, lease, epoch, observed_at: policy::instant(20) }
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
