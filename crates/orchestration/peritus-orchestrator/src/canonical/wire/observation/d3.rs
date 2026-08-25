//! D3 activation and terminal observation codecs.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};
use peritus_collaboration::CollaborationTaskId;
use peritus_scheduler::{DispatchId, WorkId, WorkerId};

use crate::{
    CollaborationChildObservation, HandoffActivationObservation, HandoffId,
    SchedulerChildObservation,
};

pub fn write_activation(
    writer: &mut CanonicalWriter,
    value: &HandoffActivationObservation,
) -> Result<(), CodecError> {
    for id in [
        value.handoff_id().as_bytes(),
        value.task_id().as_bytes(),
        value.work_id().as_bytes(),
        value.dispatch_id().as_bytes(),
        value.worker_id().as_bytes(),
        value.owner().as_bytes(),
    ] {
        super::super::write_id(writer, id)?;
    }
    writer.write_u8(super::super::harness_role_tag(value.role()))?;
    super::super::write_id(writer, value.scheduler_run_id().as_bytes())?;
    super::super::write_id(writer, value.collaboration_run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::write_head(writer, value.scheduler_head())?;
    super::write_head(writer, value.collaboration_head())
}

pub fn read_activation(
    reader: &mut CanonicalReader<'_>,
) -> Result<HandoffActivationObservation, CodecError> {
    HandoffActivationObservation::from_wire(
        nominal(reader, HandoffId::new)?,
        nominal(reader, CollaborationTaskId::new)?,
        nominal(reader, WorkId::new)?,
        nominal(reader, DispatchId::new)?,
        nominal(reader, WorkerId::new)?,
        super::super::read_actor_id(reader)?,
        super::super::read_harness_role(reader)?,
        super::super::read_run_id(reader)?,
        super::super::read_run_id(reader)?,
        super::super::read_revision(reader)?,
        super::read_head(reader)?,
        super::read_head(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

pub(super) fn write_scheduler(
    writer: &mut CanonicalWriter,
    value: &SchedulerChildObservation,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::write_head(writer, value.head())
}

pub(super) fn read_scheduler(
    reader: &mut CanonicalReader<'_>,
) -> Result<SchedulerChildObservation, CodecError> {
    SchedulerChildObservation::from_wire(
        super::super::read_run_id(reader)?,
        super::super::read_revision(reader)?,
        super::read_head(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

pub(super) fn write_collaboration(
    writer: &mut CanonicalWriter,
    value: &CollaborationChildObservation,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.run_id().as_bytes())?;
    super::super::write_revision(writer, value.revision())?;
    super::write_head(writer, value.head())
}

pub(super) fn read_collaboration(
    reader: &mut CanonicalReader<'_>,
) -> Result<CollaborationChildObservation, CodecError> {
    CollaborationChildObservation::from_wire(
        super::super::read_run_id(reader)?,
        super::super::read_revision(reader)?,
        super::read_head(reader)?,
    )
    .map_err(|_| super::super::invalid(reader))
}

fn nominal<T, E>(
    reader: &mut CanonicalReader<'_>,
    constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
) -> Result<T, CodecError> {
    let offset = reader.offset();
    constructor(reader.read_fixed()?).map_err(|_| super::super::invalid_at(offset))
}
