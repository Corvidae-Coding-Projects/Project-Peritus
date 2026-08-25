//! Canonical codecs for checked cross-aggregate observations.

mod agent;
mod d3;
mod quality;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};

use crate::child::CancellationChildClassification;
use crate::{ChildHead, ChildObservation, KernelAcceptanceObservation};

pub use d3::{read_activation, write_activation};
pub use quality::{read_gate, read_review, write_gate, write_review};

pub fn write_head(writer: &mut CanonicalWriter, value: ChildHead) -> Result<(), CodecError> {
    writer.write_u8(super::child_kind_tag(value.aggregate()))?;
    writer.write_u64(value.sequence().get())?;
    super::write_id(writer, value.last_event_id().as_bytes())?;
    super::write_digest(writer, value.state_digest())?;
    writer.write_option_tag(value.terminal().is_some())?;
    if let Some(terminal) = value.terminal() {
        writer.write_u8(super::child_terminal_tag(terminal))?;
    }
    Ok(())
}

pub fn read_head(reader: &mut CanonicalReader<'_>) -> Result<ChildHead, CodecError> {
    let aggregate = super::read_child_kind(reader)?;
    let sequence = super::read_sequence(reader)?;
    let event = super::read_event_id(reader)?;
    let digest = super::read_digest(reader)?;
    let terminal =
        reader.read_option_tag()?.then(|| super::read_child_terminal(reader)).transpose()?;
    ChildHead::new(aggregate, sequence, event, digest, terminal).map_err(|_| super::invalid(reader))
}

pub fn write_observation(
    writer: &mut CanonicalWriter,
    value: &ChildObservation,
) -> Result<(), CodecError> {
    match value {
        ChildObservation::Agent(item) => {
            writer.write_u8(1)?;
            agent::write_agent(writer, item)
        }
        ChildObservation::Gates(item) => {
            writer.write_u8(2)?;
            write_gate(writer, item)
        }
        ChildObservation::Review(item) => {
            writer.write_u8(3)?;
            write_review(writer, item)
        }
        ChildObservation::ReviewFixer(item) => {
            writer.write_u8(8)?;
            quality::write_review_fixer(writer, item)
        }
        ChildObservation::Scheduler(item) => {
            writer.write_u8(4)?;
            d3::write_scheduler(writer, item)
        }
        ChildObservation::Collaboration(item) => {
            writer.write_u8(5)?;
            d3::write_collaboration(writer, item)
        }
        ChildObservation::HandoffActivation(item) => {
            writer.write_u8(6)?;
            write_activation(writer, item)
        }
        ChildObservation::KernelAcceptance(item) => {
            writer.write_u8(7)?;
            write_kernel(writer, *item)
        }
        ChildObservation::CancellationClassification(item) => {
            writer.write_u8(9)?;
            writer.write_u8(super::child_kind_tag(item.aggregate()))?;
            super::write_revision(writer, item.revision())?;
            writer.write_u8(super::cancellation_classification_tag(item.kind()))?;
            super::write_digest(writer, item.evidence_digest())
        }
    }
}

pub fn read_observation(reader: &mut CanonicalReader<'_>) -> Result<ChildObservation, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => agent::read_agent(reader).map(ChildObservation::Agent),
        2 => read_gate(reader).map(ChildObservation::Gates),
        3 => read_review(reader).map(ChildObservation::Review),
        4 => d3::read_scheduler(reader).map(ChildObservation::Scheduler),
        5 => d3::read_collaboration(reader).map(ChildObservation::Collaboration),
        6 => read_activation(reader).map(ChildObservation::HandoffActivation),
        7 => read_kernel(reader).map(ChildObservation::KernelAcceptance),
        8 => quality::read_review_fixer(reader).map(ChildObservation::ReviewFixer),
        9 => CancellationChildClassification::from_wire(
            super::read_child_kind(reader)?,
            super::read_revision(reader)?,
            super::read_cancellation_classification(reader)?,
            super::read_digest(reader)?,
        )
        .map(ChildObservation::CancellationClassification)
        .map_err(|_| super::invalid(reader)),
        _ => Err(super::unknown(offset)),
    }
}

pub fn write_kernel(
    writer: &mut CanonicalWriter,
    value: KernelAcceptanceObservation,
) -> Result<(), CodecError> {
    super::write_id(writer, value.event_id().as_bytes())?;
    super::write_id(writer, value.command_id().as_bytes())?;
    writer.write_u64(value.sequence().get())?;
    super::write_event_option(writer, value.previous_event_id())?;
    super::write_id(writer, value.run_id().as_bytes())?;
    super::write_revision(writer, value.revision())?;
    writer.write_u8(super::kernel_outcome_tag(value.outcome()))
}

pub fn read_kernel(
    reader: &mut CanonicalReader<'_>,
) -> Result<KernelAcceptanceObservation, CodecError> {
    Ok(KernelAcceptanceObservation::from_wire(
        super::read_event_id(reader)?,
        super::read_command_id(reader)?,
        super::read_sequence(reader)?,
        super::read_event_option(reader)?,
        super::read_run_id(reader)?,
        super::read_revision(reader)?,
        super::read_kernel_outcome(reader)?,
    ))
}

pub fn bounded_count(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count <= maximum { Ok(count) } else { Err(super::invalid(reader)) }
}
