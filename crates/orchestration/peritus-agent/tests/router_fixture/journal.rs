//! Small journal append helpers for committed authority fixtures.

use std::time::Duration;

use peritus_codec::{CodecLimits, encode_frame, encode_message, sha256};
use peritus_journal::{
    AggregateHead, AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, SqliteJournal, SqliteJournalOptions, StoreId,
};
use peritus_kernel::{CommandEnvelope, KernelEvent};
use peritus_protocol::KernelEventDto;
use peritus_types::{CommandId, EventId, EventSequence, RevisionTuple, Sha256Digest};

use super::TestRoot;

pub fn open_journal(root: &TestRoot) -> SqliteJournal {
    SqliteJournal::open(
        root.path().join("authority.sqlite3"),
        StoreId::new([201; 16]).unwrap(),
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .unwrap()
}

pub fn command(seed: u8) -> CommandId {
    CommandId::new([seed; 16]).unwrap()
}
pub fn event(seed: u8) -> EventId {
    EventId::new([seed; 16]).unwrap()
}
pub const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
pub fn aggregate(kind: AggregateKind, seed: u8) -> AggregateKey {
    AggregateKey::new(kind, AggregateId::new([seed; 16]).unwrap())
}

#[allow(clippy::too_many_arguments)]
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
        encode_frame(300, 1, &[u8::try_from(sequence).unwrap()], CodecLimits::PRODUCTION).unwrap(),
    )
    .unwrap();
    let draft = EventDraft::new(
        key,
        EventSequence::new(sequence).unwrap(),
        event_id,
        previous_event_id,
        frame,
        revision_digest(revision),
        Vec::new(),
    )
    .unwrap();
    AppendRequest::new(
        StoreId::new([201; 16]).unwrap(),
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
    AggregateKey::new(AggregateKind::Kernel, AggregateId::new(*session.as_bytes()).unwrap())
}

pub fn kernel_append(
    envelope: CommandEnvelope,
    kernel_event: KernelEvent,
    head: HeadExpectation,
) -> AppendRequest {
    let key = head.key();
    let frame = ExactFrame::new(
        encode_message(&KernelEventDto::from(kernel_event), CodecLimits::PRODUCTION).unwrap(),
    )
    .unwrap();
    let draft = EventDraft::new(
        key,
        kernel_event.sequence(),
        kernel_event.id(),
        kernel_event.previous_event_id(),
        frame,
        revision_digest(kernel_event.revision()),
        Vec::new(),
    )
    .unwrap();
    AppendRequest::new(
        StoreId::new([201; 16]).unwrap(),
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
