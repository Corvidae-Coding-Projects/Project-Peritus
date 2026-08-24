//! Durable journal fixture primitives.

use std::time::Duration;

use peritus_codec::{CodecLimits, encode_frame, encode_message, sha256};
use peritus_journal::{
    AggregateHead, AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_kernel::{CommandEnvelope, KernelEvent};
use peritus_protocol::KernelEventDto;
use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, Sha256Digest};
use tempfile::TempDir;

pub fn open(temp: &TempDir) -> SqliteJournal {
    SqliteJournal::open(
        temp.path().join("authority.sqlite3"),
        store_id(),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .expect("open authority journal")
}

pub fn store_id() -> StoreId {
    StoreId::new([201; 16]).expect("store")
}

pub fn command(seed: u8) -> CommandId {
    CommandId::new([seed; 16]).expect("command")
}

pub fn event(seed: u8) -> EventId {
    EventId::new([seed; 16]).expect("event")
}

pub const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}

pub fn aggregate(kind: AggregateKind, seed: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new([seed; 16]).expect("aggregate"))
}

#[allow(clippy::too_many_arguments, reason = "journal append binds every durable identity")]
pub fn append(
    key: AggregateKey,
    command_id: CommandId,
    sequence: u64,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    head: HeadExpectation,
    revision: RevisionTuple,
) -> AppendRequest {
    let frame = ExactFrame::new(
        encode_frame(
            300,
            1,
            &[u8::try_from(sequence).expect("small sequence")],
            CodecLimits::PRODUCTION,
        )
        .expect("event frame"),
    )
    .expect("exact event frame");
    let draft = EventDraft::new(
        key,
        EventSequence::new(sequence).expect("event sequence"),
        event_id,
        previous_event_id,
        frame,
        revision_digest(revision),
        Vec::new(),
    )
    .expect("event draft");
    AppendRequest::new(
        store_id(),
        command_id,
        sha256(command_id.as_bytes()),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
}

pub fn kernel_key(session: peritus_types::SessionId) -> AggregateKey {
    AggregateKey::new(
        AggregateKind::Kernel,
        AggregateId::new(*session.as_bytes()).expect("kernel aggregate"),
    )
}

pub fn kernel_append(
    envelope: CommandEnvelope,
    event: KernelEvent,
    head: HeadExpectation,
) -> AppendRequest {
    let key = head.key();
    let frame = ExactFrame::new(
        encode_message(&KernelEventDto::from(event), CodecLimits::PRODUCTION)
            .expect("kernel event frame"),
    )
    .expect("exact kernel event");
    let draft = EventDraft::new(
        key,
        event.sequence(),
        event.id(),
        event.previous_event_id(),
        frame,
        revision_digest(event.revision()),
        Vec::new(),
    )
    .expect("kernel draft");
    AppendRequest::new(
        store_id(),
        envelope.command_id(),
        sha256(envelope.command_id().as_bytes()),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
}

pub const fn present(head: AggregateHead) -> HeadExpectation {
    HeadExpectation::Present(head)
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
