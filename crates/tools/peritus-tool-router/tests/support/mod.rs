//! Durable exact-authority fixtures for router integration tests.

#![allow(
    dead_code,
    reason = "each separately compiled integration target uses a different fixture subset"
)]

mod authority;
mod journal;
mod kernel;
mod tool;

pub use authority::{AuthorityReceipts, commit_authority, instant};
pub use journal::open_journal;
#[allow(
    unused_imports,
    reason = "each integration target consumes a different subset of the shared fixture API"
)]
pub use tool::{authority_request, call, complete_truncation, router};

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
        Self::new_named(seed, "fixture.inspect")
    }

    pub fn new_named(seed: u8, capability: &str) -> Self {
        let contract = contract_dto().try_into_domain(CodecLimits::PRODUCTION).unwrap();
        let value = |offset: u8| seed.wrapping_add(offset).max(1);
        let workspace = WorkspaceId::new([value(1); 16]).unwrap();
        let revision = RevisionTuple::new(
            contract.id(),
            HarnessId::new([value(2); 16]).unwrap(),
            workspace,
            peritus_types::Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([value(3); 16]).unwrap(),
            ProviderProfileId::new([value(4); 16]).unwrap(),
        );
        Self {
            resource: ResourceId::new([value(5); 16]).unwrap(),
            environment: EnvironmentId::new([value(6); 16]).unwrap(),
            actor: ActorId::new([value(7); 16]).unwrap(),
            session: SessionId::new([value(8); 16]).unwrap(),
            action: ActionId::new([value(9); 16]).unwrap(),
            capability: CapabilityName::new(capability.to_owned()).unwrap(),
            revision,
            project: ProjectId::new([value(11); 16]).unwrap(),
            run: RunId::new([value(12); 16]).unwrap(),
            attempt: AttemptId::new([value(13); 16]).unwrap(),
            turn: TurnId::new([value(14); 16]).unwrap(),
            kernel_root_budget: BudgetId::new([value(15); 16]).unwrap(),
            kernel_child_budget: BudgetId::new([value(16); 16]).unwrap(),
            tool_budget: BudgetId::new([value(17); 16]).unwrap(),
            reservation: BudgetReservationId::new([value(18); 16]).unwrap(),
        }
    }
}

fn contract_dto() -> AcceptanceContractDto {
    let current = std::env::current_dir().unwrap();
    let path = current
        .ancestors()
        .map(|root| root.join("protocol/fixtures/v1/acceptance-contract.bin"))
        .find(|path| path.is_file())
        .expect("checked-in acceptance contract path");
    let bytes = fs::read(path).unwrap();
    peritus_codec::decode_message(&bytes, CodecLimits::PRODUCTION).unwrap()
}
