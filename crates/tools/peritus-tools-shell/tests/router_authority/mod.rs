//! Local C4 router identity and committed-authority fixtures for the integration target.

mod committed;
mod journal;
mod kernel;

pub use committed::{AuthorityReceipts, commit_authority};
pub use journal::open as open_journal;

use std::{fs, path::Path};

use peritus_codec::CodecLimits;
use peritus_protocol::AcceptanceContractDto;
use peritus_types::{
    ActionId, ActorId, AttemptId, BudgetId, BudgetReservationId, CapabilityName, EnvironmentId,
    HarnessId, PolicyId, ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple,
    RunId, SessionId, TurnId, WorkspaceId,
};
use tempfile::TempDir;

pub struct TestRoot(TempDir);

impl TestRoot {
    pub fn new() -> Self {
        Self(TempDir::new().expect("temporary router root"))
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

pub struct Ids {
    pub resource: ResourceId,
    pub environment: EnvironmentId,
    pub actor: ActorId,
    pub session: SessionId,
    pub action: ActionId,
    pub capability: CapabilityName,
    pub revision: RevisionTuple,
    pub project: ProjectId,
    pub run: RunId,
    pub attempt: AttemptId,
    pub turn: TurnId,
    pub kernel_root_budget: BudgetId,
    pub kernel_child_budget: BudgetId,
    pub tool_budget: BudgetId,
    pub reservation: BudgetReservationId,
}

impl Ids {
    pub fn new(seed: u8) -> Self {
        let contract =
            contract_dto().try_into_domain(CodecLimits::PRODUCTION).expect("acceptance contract");
        let value = |offset: u8| seed.wrapping_add(offset).max(1);
        let workspace = WorkspaceId::new([value(1); 16]).expect("workspace");
        let revision = RevisionTuple::new(
            contract.id(),
            HarnessId::new([value(2); 16]).expect("harness"),
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([value(3); 16]).expect("policy"),
            ProviderProfileId::new([value(4); 16]).expect("provider"),
        );
        Self {
            resource: ResourceId::new([value(5); 16]).expect("resource"),
            environment: EnvironmentId::new([value(6); 16]).expect("environment"),
            actor: ActorId::new([value(7); 16]).expect("actor"),
            session: SessionId::new([value(8); 16]).expect("session"),
            action: ActionId::new([value(9); 16]).expect("action"),
            capability: CapabilityName::new("fixture.inspect".to_owned()).expect("capability"),
            revision,
            project: ProjectId::new([value(11); 16]).expect("project"),
            run: RunId::new([value(12); 16]).expect("run"),
            attempt: AttemptId::new([value(13); 16]).expect("attempt"),
            turn: TurnId::new([value(14); 16]).expect("turn"),
            kernel_root_budget: BudgetId::new([value(15); 16]).expect("root budget"),
            kernel_child_budget: BudgetId::new([value(16); 16]).expect("child budget"),
            tool_budget: BudgetId::new([value(17); 16]).expect("tool budget"),
            reservation: BudgetReservationId::new([value(18); 16]).expect("reservation"),
        }
    }
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
