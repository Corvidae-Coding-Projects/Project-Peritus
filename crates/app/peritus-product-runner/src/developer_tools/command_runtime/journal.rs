//! Small exact-journal helpers for per-command authority evidence.

use std::{path::Path, time::Duration};

use peritus_codec::{CodecLimits, encode_frame, encode_message, sha256};
use peritus_journal::{
    AggregateHead, AggregateKey, AppendRequest, EventDraft, ExactFrame, HeadExpectation,
    SqliteJournal, SqliteJournalOptions,
};
use peritus_kernel::{CommandEnvelope, KernelEvent};
use peritus_protocol::KernelEventDto;
use peritus_types::{EventSequence, RevisionTuple, Sha256Digest};

use super::identity::CommandIds;

pub(super) fn open(path: &Path, ids: &CommandIds, label: &str) -> Result<SqliteJournal, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create command authority directory: {error}"))?;
    }
    SqliteJournal::open(
        path,
        ids.store(label)?,
        SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
    )
    .map_err(|error| format!("open command authority journal: {error}"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append(
    ids: &CommandIds,
    store_label: &str,
    key: AggregateKey,
    command_label: &str,
    sequence: u64,
    event_label: &str,
    previous: Option<peritus_types::EventId>,
    head: HeadExpectation,
) -> Result<AppendRequest, String> {
    let command_id = ids.command(command_label)?;
    let event_id = ids.event(event_label)?;
    let frame = ExactFrame::new(
        encode_frame(300, 1, &[u8::try_from(sequence).unwrap_or(u8::MAX)], CodecLimits::PRODUCTION)
            .map_err(|error| format!("encode command authority frame: {error}"))?,
    )
    .map_err(|error| format!("construct command authority frame: {error}"))?;
    let draft = EventDraft::new(
        key,
        EventSequence::new(sequence)
            .map_err(|error| format!("construct command event sequence: {error:?}"))?,
        event_id,
        previous,
        frame,
        revision_digest(ids.revision),
        Vec::new(),
    )
    .map_err(|error| format!("construct command event draft: {error}"))?;
    Ok(AppendRequest::new(
        ids.store(store_label)?,
        command_id,
        sha256(command_id.as_bytes()),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    ))
}

pub(super) fn kernel_append(
    ids: &CommandIds,
    store_label: &str,
    envelope: CommandEnvelope,
    event: KernelEvent,
    head: HeadExpectation,
) -> Result<AppendRequest, String> {
    let key = head.key();
    let frame = ExactFrame::new(
        encode_message(&KernelEventDto::from(event), CodecLimits::PRODUCTION)
            .map_err(|error| format!("encode command kernel event: {error}"))?,
    )
    .map_err(|error| format!("construct command kernel frame: {error}"))?;
    let draft = EventDraft::new(
        key,
        event.sequence(),
        event.id(),
        event.previous_event_id(),
        frame,
        revision_digest(event.revision()),
        Vec::new(),
    )
    .map_err(|error| format!("construct command kernel draft: {error}"))?;
    Ok(AppendRequest::new(
        ids.store(store_label)?,
        envelope.command_id(),
        sha256(envelope.command_id().as_bytes()),
        vec![head],
        vec![draft],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    ))
}

pub(super) const fn present(head: AggregateHead) -> HeadExpectation {
    HeadExpectation::Present(head)
}

pub(super) fn revision_digest(revision: RevisionTuple) -> Sha256Digest {
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
