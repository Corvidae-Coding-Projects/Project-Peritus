//! Public B3 scheduler command fixtures used through A3.

use std::io;

use peritus_app_protocol::{
    AppProtocolLimits, CommandBinding, CommandSubmissionFrames, CorrelationId, IdempotencyKey,
    RequestId,
};
use peritus_codec::{CodecLimits, encode_message};
use peritus_kernel::CommandEnvelope;
use peritus_protocol::CommandEnvelopeDto;
use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerId, SchedulerLimits,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, SessionId, Sha256Digest, WorkspaceId,
};

pub(super) struct SchedulerFixture {
    binding: CommandBinding,
    run_id: RunId,
    revision: RevisionTuple,
    event_id: EventId,
}

impl SchedulerFixture {
    pub(super) fn binding(&self) -> &CommandBinding {
        &self.binding
    }
}

pub(super) fn genesis(
    session: SessionId,
    seed: u8,
    key: &[u8],
    actor_byte: u8,
) -> io::Result<SchedulerFixture> {
    let revision = revision();
    let run_id = id(seed.wrapping_add(1), RunId::new)?;
    let scheduler = SchedulerBinding::new(
        run_id,
        id(seed.wrapping_add(2), SchedulerId::new)?,
        revision,
        scheduler_limits()?,
        resources()?,
    )
    .map_err(super::debug_error)?;
    let command_id = id(seed.wrapping_add(3), CommandId::new)?;
    let event_id = id(seed.wrapping_add(4), EventId::new)?;
    let command = SchedulerCommand::new(
        command_id,
        event_id,
        run_id,
        0,
        None,
        Sha256Digest::new([0; 32]),
        revision,
        SchedulerCommandKind::StartScheduler { binding: scheduler },
    )
    .map_err(super::debug_error)?;
    let binding = app_binding(session, seed, key, actor_byte, revision, &command)?;
    Ok(SchedulerFixture { binding, run_id, revision, event_id })
}

pub(super) fn stale_successor(
    session: SessionId,
    genesis: &SchedulerFixture,
    seed: u8,
    key: &[u8],
) -> io::Result<SchedulerFixture> {
    let command_id = id(seed.wrapping_add(3), CommandId::new)?;
    let event_id = id(seed.wrapping_add(4), EventId::new)?;
    let command = SchedulerCommand::new(
        command_id,
        event_id,
        genesis.run_id,
        2,
        Some(genesis.event_id),
        Sha256Digest::new([0; 32]),
        genesis.revision,
        SchedulerCommandKind::PauseScheduler,
    )
    .map_err(super::debug_error)?;
    let binding = app_binding(session, seed, key, 0x22, genesis.revision, &command)?;
    Ok(SchedulerFixture { binding, run_id: genesis.run_id, revision: genesis.revision, event_id })
}

fn app_binding(
    session: SessionId,
    seed: u8,
    key: &[u8],
    actor_byte: u8,
    revision: RevisionTuple,
    command: &SchedulerCommand,
) -> io::Result<CommandBinding> {
    let envelope = CommandEnvelope::new(
        command.command_id(),
        command.event_id(),
        command.expected_previous_event(),
        command.revision(),
    );
    let envelope_bytes =
        encode_message(&CommandEnvelopeDto::from(envelope), CodecLimits::PRODUCTION)
            .map_err(super::debug_error)?;
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(command), CodecLimits::PRODUCTION)
            .map_err(super::debug_error)?;
    let frames = CommandSubmissionFrames::parse(
        envelope_bytes,
        command_bytes,
        AppProtocolLimits::PRODUCTION,
    )
    .map_err(super::debug_error)?;
    CommandBinding::new(
        ActorId::new([actor_byte; 16]).map_err(super::debug_error)?,
        session,
        RequestId::new([seed.wrapping_add(10); 16]).map_err(super::debug_error)?,
        CorrelationId::new([seed.wrapping_add(20); 16]).map_err(super::debug_error)?,
        IdempotencyKey::new(key.to_vec()).map_err(super::debug_error)?,
        Some(revision),
        frames,
    )
    .map_err(super::debug_error)
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([10; 16]).expect("fixed acceptance identity"),
        HarnessId::new([11; 16]).expect("fixed harness identity"),
        WorkspaceId::new([12; 16]).expect("fixed workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([13; 16]).expect("fixed policy identity"),
        ProviderProfileId::new([14; 16]).expect("fixed provider identity"),
    )
}

fn scheduler_limits() -> io::Result<SchedulerLimits> {
    SchedulerLimits::new(128, 512, 16, 16, 8, 16, 4, 2, 8, 1_048_576, 4_194_304)
        .map_err(super::debug_error)
}

fn resources() -> io::Result<ResourceVector> {
    ResourceVector::new(
        vec![
            ResourceEntry::new(
                ResourceKind::CPU,
                ResourceQuantity::new(8).map_err(super::debug_error)?,
            ),
            ResourceEntry::new(
                ResourceKind::MEMORY_BYTES,
                ResourceQuantity::new(8_192).map_err(super::debug_error)?,
            ),
        ],
        8,
    )
    .map_err(super::debug_error)
}

fn id<T, E>(seed: u8, constructor: impl FnOnce([u8; 16]) -> Result<T, E>) -> io::Result<T>
where
    E: std::fmt::Debug,
{
    constructor([seed; 16]).map_err(super::debug_error)
}
