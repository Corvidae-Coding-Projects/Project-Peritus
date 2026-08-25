//! D0 terminal observation codec.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError};
use peritus_collaboration::CollaborationTaskId;
use peritus_scheduler::WorkId;
use peritus_types::TurnId;

use crate::{AgentChildObservation, FixerResponseIdentity, HandoffId, OrchestratorLimits};

pub(super) fn write_agent(
    writer: &mut CanonicalWriter,
    value: &AgentChildObservation,
) -> Result<(), CodecError> {
    for id in [
        value.handoff_id().as_bytes(),
        value.task_id().as_bytes(),
        value.work_id().as_bytes(),
        value.turn_id().as_bytes(),
        value.run_id().as_bytes(),
        value.actor().as_bytes(),
        value.attempt_id().as_bytes(),
    ] {
        super::super::write_id(writer, id)?;
    }
    writer.write_u8(super::super::harness_role_tag(value.role()))?;
    super::super::write_revision(writer, value.revision())?;
    writer.write_option_tag(value.proposal_digest().is_some())?;
    if let Some(digest) = value.proposal_digest() {
        super::super::write_digest(writer, digest)?;
    }
    writer.write_collection_len(value.fixer_responses().len())?;
    for response in value.fixer_responses() {
        super::super::write_id(writer, response.finding_id().as_bytes())?;
        super::super::write_digest(writer, response.response_digest())?;
    }
    super::write_head(writer, value.head())
}

pub(super) fn read_agent(
    reader: &mut CanonicalReader<'_>,
) -> Result<AgentChildObservation, CodecError> {
    let handoff = nominal(reader, HandoffId::new)?;
    let task = nominal(reader, CollaborationTaskId::new)?;
    let work = nominal(reader, WorkId::new)?;
    let turn = nominal(reader, TurnId::new)?;
    let run = super::super::read_run_id(reader)?;
    let actor = super::super::read_actor_id(reader)?;
    let attempt = super::super::read_attempt_id(reader)?;
    let role = super::super::read_harness_role(reader)?;
    let revision = super::super::read_revision(reader)?;
    let proposal =
        reader.read_option_tag()?.then(|| super::super::read_digest(reader)).transpose()?;
    let count =
        super::bounded_count(reader, usize::from(OrchestratorLimits::MAX_ARTIFACT_REFERENCES))?;
    let mut responses = Vec::with_capacity(count);
    for _ in 0..count {
        responses.push(FixerResponseIdentity::from_wire(
            super::super::read_finding_id(reader)?,
            super::super::read_digest(reader)?,
        ));
    }
    AgentChildObservation::from_wire(
        handoff,
        task,
        work,
        turn,
        run,
        actor,
        role,
        attempt,
        revision,
        proposal,
        responses,
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
