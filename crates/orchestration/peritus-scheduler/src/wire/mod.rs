//! D3-owned typed B3 codecs for reserved families 70, 71, and 72.

mod command;
mod domain;
mod event;
mod records;
mod state;

#[cfg(test)]
mod fixture_tests;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{
    AcceptanceSpecId, ActorId, BudgetReservationId, CommandId, EventId, Generation, HarnessId,
    PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};

use crate::{
    DispatchId, ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerBinding,
    SchedulerId, SchedulerLimits, WorkId, WorkerId,
};

use domain::{
    failure_disposition_tag, read_descriptor, read_reservation, read_spec, write_descriptor,
    write_reservation, write_spec,
};
use records::{
    read_terminal, read_work_record, read_worker_record, scheduler_phase_tag, write_terminal,
    write_work_record, write_worker_record,
};

pub use command::SchedulerCommandFrame;
pub use event::SchedulerEventFrame;
pub use state::SchedulerStateFrame;

pub const fn invalid(reader: &CanonicalReader<'_>) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, reader.offset())
}
pub const fn unknown(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::UnknownTag, offset)
}
pub fn write_id(writer: &mut CanonicalWriter, bytes: &[u8; 16]) -> Result<(), CodecError> {
    writer.write_fixed(bytes)
}
fn read_nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    construct: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    construct(reader.read_fixed()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
pub fn read_command_id(reader: &mut CanonicalReader<'_>) -> Result<CommandId, CodecError> {
    read_nominal(reader, CommandId::new)
}
pub fn read_event_id(reader: &mut CanonicalReader<'_>) -> Result<EventId, CodecError> {
    read_nominal(reader, EventId::new)
}
pub fn read_run_id(reader: &mut CanonicalReader<'_>) -> Result<RunId, CodecError> {
    read_nominal(reader, RunId::new)
}
pub fn read_actor_id(reader: &mut CanonicalReader<'_>) -> Result<ActorId, CodecError> {
    read_nominal(reader, ActorId::new)
}
pub fn read_scheduler_id(reader: &mut CanonicalReader<'_>) -> Result<SchedulerId, CodecError> {
    read_nominal(reader, SchedulerId::new)
}
pub fn read_work_id(reader: &mut CanonicalReader<'_>) -> Result<WorkId, CodecError> {
    read_nominal(reader, WorkId::new)
}
pub fn read_worker_id(reader: &mut CanonicalReader<'_>) -> Result<WorkerId, CodecError> {
    read_nominal(reader, WorkerId::new)
}
pub fn read_dispatch_id(reader: &mut CanonicalReader<'_>) -> Result<DispatchId, CodecError> {
    read_nominal(reader, DispatchId::new)
}
pub fn read_budget_id(reader: &mut CanonicalReader<'_>) -> Result<BudgetReservationId, CodecError> {
    read_nominal(reader, BudgetReservationId::new)
}
pub fn write_digest(writer: &mut CanonicalWriter, value: Sha256Digest) -> Result<(), CodecError> {
    writer.write_fixed(value.as_bytes())
}
pub fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, CodecError> {
    Ok(Sha256Digest::new(reader.read_fixed()?))
}
pub fn write_revision(
    writer: &mut CanonicalWriter,
    value: RevisionTuple,
) -> Result<(), CodecError> {
    write_id(writer, value.acceptance_spec_id().as_bytes())?;
    write_id(writer, value.harness_id().as_bytes())?;
    write_id(writer, value.workspace_id().as_bytes())?;
    writer.write_u64(value.workspace_generation().get())?;
    writer.write_u64(value.workspace_revision().get())?;
    write_id(writer, value.policy_id().as_bytes())?;
    write_id(writer, value.provider_profile_id().as_bytes())
}
pub fn read_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, CodecError> {
    let acceptance = read_nominal(reader, AcceptanceSpecId::new)?;
    let harness = read_nominal(reader, HarnessId::new)?;
    let workspace = read_nominal(reader, WorkspaceId::new)?;
    let generation_offset = reader.offset();
    let generation = Generation::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, generation_offset))?;
    let revision_offset = reader.offset();
    let revision = RevisionNumber::new(reader.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, revision_offset))?;
    Ok(RevisionTuple::new(
        acceptance,
        harness,
        workspace,
        generation,
        revision,
        read_nominal(reader, PolicyId::new)?,
        read_nominal(reader, ProviderProfileId::new)?,
    ))
}
pub fn write_option_id<T>(
    writer: &mut CanonicalWriter,
    value: Option<T>,
    bytes: impl FnOnce(T) -> [u8; 16],
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        write_id(writer, &bytes(value))?;
    }
    Ok(())
}

pub fn write_limits(
    writer: &mut CanonicalWriter,
    value: SchedulerLimits,
) -> Result<(), CodecError> {
    writer.write_u32(value.queued_work())?;
    writer.write_u32(value.retained_work())?;
    writer.write_u16(value.workers())?;
    writer.write_u16(value.dependencies_per_work())?;
    writer.write_u16(value.resource_dimensions())?;
    writer.write_u16(value.active_reservations())?;
    writer.write_u16(value.attempts_per_work())?;
    writer.write_u16(value.bypass_count())?;
    writer.write_u16(value.dispatch_batch_size())?;
    writer.write_u64(value.payload_bytes())?;
    writer.write_u64(value.state_bytes())
}
pub fn read_limits(reader: &mut CanonicalReader<'_>) -> Result<SchedulerLimits, CodecError> {
    let offset = reader.offset();
    SchedulerLimits::new(
        reader.read_u32()?,
        reader.read_u32()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u64()?,
        reader.read_u64()?,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
pub const fn production_limits() -> SchedulerLimits {
    SchedulerLimits::from_wire(
        SchedulerLimits::MAX_QUEUED_WORK,
        SchedulerLimits::MAX_RETAINED_WORK,
        SchedulerLimits::MAX_WORKERS,
        SchedulerLimits::MAX_DEPENDENCIES,
        SchedulerLimits::MAX_RESOURCE_DIMENSIONS,
        SchedulerLimits::MAX_ACTIVE_RESERVATIONS,
        SchedulerLimits::MAX_ATTEMPTS,
        SchedulerLimits::MAX_BYPASS_COUNT,
        SchedulerLimits::MAX_DISPATCH_BATCH,
        SchedulerLimits::MAX_PAYLOAD_BYTES,
        SchedulerLimits::MAX_STATE_BYTES,
    )
}

pub fn write_resources(
    writer: &mut CanonicalWriter,
    value: &ResourceVector,
) -> Result<(), CodecError> {
    writer.write_collection_len(value.entries().len())?;
    for entry in value.entries() {
        writer.write_u16(entry.kind().tag())?;
        writer.write_u64(entry.quantity().get())?;
    }
    Ok(())
}
pub fn read_resources(
    reader: &mut CanonicalReader<'_>,
    maximum: u16,
) -> Result<ResourceVector, CodecError> {
    let offset = reader.offset();
    let count = reader.read_collection_len()?;
    if count == 0 || count > usize::from(maximum) {
        return Err(invalid(reader));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = ResourceKind::new(reader.read_u16()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
        let quantity = ResourceQuantity::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
        entries.push(ResourceEntry::new(kind, quantity));
    }
    ResourceVector::new(entries, maximum)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_binding(
    writer: &mut CanonicalWriter,
    value: &SchedulerBinding,
) -> Result<(), CodecError> {
    write_id(writer, value.run_id().as_bytes())?;
    write_id(writer, value.scheduler_id().as_bytes())?;
    write_revision(writer, value.revision())?;
    write_limits(writer, value.limits())?;
    write_resources(writer, value.capacity())
}
pub fn read_binding(reader: &mut CanonicalReader<'_>) -> Result<SchedulerBinding, CodecError> {
    let offset = reader.offset();
    let run = read_run_id(reader)?;
    let scheduler = read_scheduler_id(reader)?;
    let revision = read_revision(reader)?;
    let limits = read_limits(reader)?;
    SchedulerBinding::new(
        run,
        scheduler,
        revision,
        limits,
        read_resources(reader, limits.resource_dimensions())?,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}
