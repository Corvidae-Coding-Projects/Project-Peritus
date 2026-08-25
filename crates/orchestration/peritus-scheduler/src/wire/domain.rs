//! Wire encoding for worker, work, and reservation domain values.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::BudgetReservationId;

use crate::{
    AttemptNumber, ExecutionClass, FailureDisposition, RecoveryPolicy, SchedulerLimits,
    SchedulerReservation, WorkId, WorkSpec, WorkerDescriptor,
};

pub fn write_descriptor(
    writer: &mut CanonicalWriter,
    value: &WorkerDescriptor,
) -> Result<(), CodecError> {
    super::write_id(writer, value.id().as_bytes())?;
    super::write_id(writer, value.owner().as_bytes())?;
    writer.write_collection_len(value.classes().len())?;
    for class in value.classes() {
        writer.write_u8(execution_class_tag(*class))?;
    }
    super::write_resources(writer, value.capacity())?;
    writer.write_u16(value.concurrency())
}

pub fn read_descriptor(
    reader: &mut CanonicalReader<'_>,
    limits: SchedulerLimits,
) -> Result<WorkerDescriptor, CodecError> {
    let offset = reader.offset();
    let id = super::read_worker_id(reader)?;
    let owner = super::read_actor_id(reader)?;
    let count = reader.read_collection_len()?;
    if count == 0 || count > 5 {
        return Err(super::invalid(reader));
    }
    let mut classes = Vec::with_capacity(count);
    for _ in 0..count {
        classes.push(read_class(reader)?);
    }
    WorkerDescriptor::new(
        id,
        owner,
        classes,
        super::read_resources(reader, limits.resource_dimensions())?,
        reader.read_u16()?,
        limits,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_spec(writer: &mut CanonicalWriter, value: &WorkSpec) -> Result<(), CodecError> {
    super::write_id(writer, value.id().as_bytes())?;
    super::write_id(writer, value.owner().as_bytes())?;
    super::write_revision(writer, value.revision())?;
    writer.write_u8(execution_class_tag(value.class()))?;
    writer.write_u8(value.priority())?;
    super::write_resources(writer, value.request())?;
    super::write_option_id(writer, value.budget_reservation(), BudgetReservationId::into_bytes)?;
    writer.write_collection_len(value.dependencies().len())?;
    for dependency in value.dependencies() {
        super::write_id(writer, dependency.as_bytes())?;
    }
    super::write_option_id(writer, value.parent(), WorkId::into_bytes)?;
    writer.write_u16(value.maximum_attempts().get())?;
    writer.write_u8(recovery_policy_tag(value.recovery()))?;
    super::write_digest(writer, value.payload_digest())
}

pub fn read_spec(
    reader: &mut CanonicalReader<'_>,
    limits: SchedulerLimits,
) -> Result<WorkSpec, CodecError> {
    let offset = reader.offset();
    let id = super::read_work_id(reader)?;
    let owner = super::read_actor_id(reader)?;
    let revision = super::read_revision(reader)?;
    let class = read_class(reader)?;
    let priority = reader.read_u8()?;
    let request = super::read_resources(reader, limits.resource_dimensions())?;
    let budget = reader.read_option_tag()?.then(|| super::read_budget_id(reader)).transpose()?;
    let count = reader.read_collection_len()?;
    if count > usize::from(limits.dependencies_per_work()) {
        return Err(super::invalid(reader));
    }
    let mut dependencies = Vec::with_capacity(count);
    for _ in 0..count {
        dependencies.push(super::read_work_id(reader)?);
    }
    let parent = reader.read_option_tag()?.then(|| super::read_work_id(reader)).transpose()?;
    let attempt_offset = reader.offset();
    let attempts = AttemptNumber::new(reader.read_u16()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, attempt_offset))?;
    WorkSpec::new(
        id,
        owner,
        revision,
        class,
        priority,
        request,
        budget,
        dependencies,
        parent,
        attempts,
        read_recovery(reader)?,
        super::read_digest(reader)?,
        limits,
    )
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub fn write_reservation(
    writer: &mut CanonicalWriter,
    value: &SchedulerReservation,
) -> Result<(), CodecError> {
    super::write_id(writer, value.work_id().as_bytes())?;
    super::write_id(writer, value.dispatch_id().as_bytes())?;
    super::write_id(writer, value.worker_id().as_bytes())?;
    super::write_id(writer, value.owner().as_bytes())?;
    writer.write_u16(value.attempt().get())?;
    super::write_revision(writer, value.revision())?;
    super::write_resources(writer, value.resources())?;
    super::write_digest(writer, value.dispatch_token())?;
    writer.write_bool(value.started())
}

pub fn read_reservation(
    reader: &mut CanonicalReader<'_>,
    limits: SchedulerLimits,
) -> Result<SchedulerReservation, CodecError> {
    let work = super::read_work_id(reader)?;
    let dispatch = super::read_dispatch_id(reader)?;
    let worker = super::read_worker_id(reader)?;
    let owner = super::read_actor_id(reader)?;
    let offset = reader.offset();
    let attempt = AttemptNumber::new(reader.read_u16()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))?;
    Ok(SchedulerReservation::from_wire(
        work,
        dispatch,
        worker,
        owner,
        attempt,
        super::read_revision(reader)?,
        super::read_resources(reader, limits.resource_dimensions())?,
        super::read_digest(reader)?,
        reader.read_bool()?,
    ))
}

fn read_class(reader: &mut CanonicalReader<'_>) -> Result<ExecutionClass, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ExecutionClass::Model),
        2 => Ok(ExecutionClass::Tool),
        3 => Ok(ExecutionClass::Gate),
        4 => Ok(ExecutionClass::Review),
        5 => Ok(ExecutionClass::Coordination),
        _ => Err(super::unknown(offset)),
    }
}

fn read_recovery(reader: &mut CanonicalReader<'_>) -> Result<RecoveryPolicy, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(RecoveryPolicy::RetrySafe),
        2 => Ok(RecoveryPolicy::Ambiguous),
        3 => Ok(RecoveryPolicy::Fail),
        _ => Err(super::unknown(offset)),
    }
}

const fn execution_class_tag(value: ExecutionClass) -> u8 {
    match value {
        ExecutionClass::Model => 1,
        ExecutionClass::Tool => 2,
        ExecutionClass::Gate => 3,
        ExecutionClass::Review => 4,
        ExecutionClass::Coordination => 5,
    }
}

const fn recovery_policy_tag(value: RecoveryPolicy) -> u8 {
    match value {
        RecoveryPolicy::RetrySafe => 1,
        RecoveryPolicy::Ambiguous => 2,
        RecoveryPolicy::Fail => 3,
    }
}

pub const fn failure_disposition_tag(value: FailureDisposition) -> u8 {
    match value {
        FailureDisposition::Retryable => 1,
        FailureDisposition::Failed => 2,
        FailureDisposition::Ambiguous => 3,
    }
}
