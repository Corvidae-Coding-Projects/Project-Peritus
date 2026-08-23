#![allow(
    dead_code,
    reason = "shared integration support is compiled independently for each focused test binary"
)]

pub mod b1;

use std::{fs, time::Duration};

use peritus_codec::{CodecLimits, encode_frame, encode_message, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CredentialRegistryInstall, EventDraft,
    ExactFrame, HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_kernel::{CommandEnvelope, KernelEvent};
use peritus_protocol::{AcceptanceContractDto, KernelEventDto};
use peritus_test_support::DeterministicIdSource;
use peritus_types::{
    AcceptanceSpecId, CommandId, EnvironmentId, EventId, Generation, HarnessId, PolicyId,
    ProjectId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, SessionId,
    Sha256Digest, WorkspaceId,
};
use tempfile::TempDir;

pub struct DomainIds {
    source: DeterministicIdSource,
    pub acceptance: AcceptanceSpecId,
    pub harness: HarnessId,
    pub workspace: WorkspaceId,
    pub policy: PolicyId,
    pub provider: ProviderProfileId,
    pub environment: EnvironmentId,
    pub resource: ResourceId,
    pub project: ProjectId,
    pub session: SessionId,
}

impl DomainIds {
    pub fn new(namespace: [u8; 8]) -> Self {
        let mut source = DeterministicIdSource::new(namespace);
        Self {
            acceptance: source.next(AcceptanceSpecId::new).expect("acceptance id"),
            harness: source.next(HarnessId::new).expect("harness id"),
            workspace: source.next(WorkspaceId::new).expect("workspace id"),
            policy: source.next(PolicyId::new).expect("policy id"),
            provider: source.next(ProviderProfileId::new).expect("provider id"),
            environment: source.next(EnvironmentId::new).expect("environment id"),
            resource: source.next(ResourceId::new).expect("resource id"),
            project: source.next(ProjectId::new).expect("project id"),
            session: source.next(SessionId::new).expect("session id"),
            source,
        }
    }

    pub const fn revision(&self) -> RevisionTuple {
        RevisionTuple::new(
            self.acceptance,
            self.harness,
            self.workspace,
            Generation::first(),
            RevisionNumber::first(),
            self.policy,
            self.provider,
        )
    }

    pub fn next<T>(
        &mut self,
        constructor: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
    ) -> T {
        self.source.next(constructor).expect("deterministic domain id")
    }
}

pub fn store_id() -> StoreId {
    StoreId::new([0x51; 16]).expect("journal store id")
}

pub fn open(temp: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        temp.path().join("journal.sqlite3"),
        store_id(),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("open journal")
}

pub fn command(value: u8) -> CommandId {
    CommandId::new([value; 16]).expect("command id")
}

pub fn event(value: u8) -> EventId {
    EventId::new([value; 16]).expect("event id")
}

pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

pub fn aggregate(kind: AggregateKind, value: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new([value; 16]).expect("aggregate id"))
}

pub fn frame(value: u8) -> ExactFrame {
    ExactFrame::new(
        encode_frame(300, 1, &[value, value.wrapping_add(1)], CodecLimits::PRODUCTION)
            .expect("canonical frame"),
    )
    .expect("exact frame")
}

#[allow(clippy::too_many_arguments, reason = "journal event identity stays explicit")]
pub fn append_request(
    command_id: CommandId,
    request_digest: Sha256Digest,
    head: HeadExpectation,
    sequence: u64,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    frame: ExactFrame,
    revision_digest: Sha256Digest,
) -> AppendRequest {
    let key = head.key();
    let draft = EventDraft::new(
        key,
        peritus_types::EventSequence::new(sequence).expect("positive event sequence"),
        event_id,
        previous_event_id,
        frame,
        revision_digest,
        Vec::new(),
    )
    .expect("event draft");
    AppendRequest::new(
        store_id(),
        command_id,
        request_digest,
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
}

pub fn kernel_key(session_id: SessionId) -> AggregateKey {
    AggregateKey::new(
        AggregateKind::Kernel,
        AggregateId::new(*session_id.as_bytes()).expect("session aggregate id"),
    )
}

pub fn kernel_append(
    envelope: CommandEnvelope,
    event: KernelEvent,
    head: HeadExpectation,
) -> AppendRequest {
    let frame = ExactFrame::new(
        encode_message(&KernelEventDto::from(event), CodecLimits::PRODUCTION)
            .expect("canonical kernel event"),
    )
    .expect("exact kernel event frame");
    append_request(
        envelope.command_id(),
        sha256(envelope.command_id().as_bytes()),
        head,
        event.sequence().get(),
        event.id(),
        event.previous_event_id(),
        frame,
        revision_digest(event.revision()),
    )
}

#[allow(clippy::too_many_arguments, reason = "registry event identity stays explicit")]
pub fn registry_plan(
    aggregate: AggregateKey,
    head: HeadExpectation,
    command_id: CommandId,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    sequence: u64,
    install: CredentialRegistryInstall,
) -> peritus_journal::AppendPlan {
    let draft = EventDraft::new(
        aggregate,
        peritus_types::EventSequence::new(sequence).expect("registry event sequence"),
        event_id,
        previous_event_id,
        frame(u8::try_from(sequence).expect("small registry event sequence")),
        digest(200),
        Vec::new(),
    )
    .expect("registry event");
    AppendRequest::new(
        store_id(),
        command_id,
        digest(*command_id.as_bytes().first().expect("command byte")),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        Some(install),
        Vec::new(),
    )
    .plan()
    .expect("registry append plan")
}

pub fn contract_dto() -> AcceptanceContractDto {
    let current = std::env::current_dir().expect("journal test working directory");
    let fixture = current
        .ancestors()
        .map(|root| root.join("protocol/fixtures/v1/acceptance-contract.bin"))
        .find(|path| path.is_file())
        .expect("checked-in acceptance contract fixture path");
    let bytes = fs::read(fixture).expect("read checked-in acceptance contract fixture");
    peritus_codec::decode_message(&bytes, CodecLimits::PRODUCTION)
        .expect("checked-in acceptance contract fixture")
}

pub const fn revision_for_contract(
    contract_id: AcceptanceSpecId,
    ids: &DomainIds,
) -> RevisionTuple {
    RevisionTuple::new(
        contract_id,
        ids.harness,
        ids.workspace,
        Generation::first(),
        RevisionNumber::first(),
        ids.policy,
        ids.provider,
    )
}

fn revision_digest(revision: RevisionTuple) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(112);
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
    sha256(&bytes)
}
