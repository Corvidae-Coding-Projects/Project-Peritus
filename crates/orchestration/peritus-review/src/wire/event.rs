use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::EventSequence;

use crate::{ReviewEvent, ReviewEventKind, ReviewLimits};

/// Canonical family-54 schema-v1 review event frame.
pub struct ReviewEventFrame(pub ReviewEvent);

impl ReviewEventFrame {
    pub fn into_event(self) -> ReviewEvent {
        self.0
    }
}

impl CanonicalEncode for ReviewEventFrame {
    const FAMILY: u16 = 54;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let event = &self.0;
        super::write_id(writer, event.id().as_bytes())?;
        super::write_id(writer, event.command_id().as_bytes())?;
        writer.write_u64(event.sequence().get())?;
        super::write_option_id(writer, event.previous_event(), peritus_types::EventId::into_bytes)?;
        super::write_id(writer, event.run_id().as_bytes())?;
        super::write_revision(writer, event.revision())?;
        super::write_digest(writer, event.prior_state_digest())?;
        super::write_digest(writer, event.successor_state_digest())?;
        write_kind(writer, event.kind())
    }
}

impl CanonicalDecode for ReviewEventFrame {
    const FAMILY: u16 = 54;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let id = super::read_event_id(reader)?;
        let command_id = super::read_command_id(reader)?;
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (sequence.get() == 1) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        Ok(Self(ReviewEvent::from_wire(
            id,
            command_id,
            sequence,
            previous,
            super::read_run_id(reader)?,
            super::read_revision(reader)?,
            super::read_digest(reader)?,
            super::read_digest(reader)?,
            read_kind(reader)?,
        )))
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed event tag table is kept contiguous for wire-schema review"
)]
fn write_kind(writer: &mut CanonicalWriter, kind: &ReviewEventKind) -> Result<(), CodecError> {
    match kind {
        ReviewEventKind::RunStarted { binding, limits } => {
            writer.write_u8(1)?;
            super::write_binding(writer, binding)?;
            super::write_limits(writer, *limits)?;
        }
        ReviewEventKind::RevisionAdvanced { binding } => {
            writer.write_u8(2)?;
            super::write_binding(writer, binding)?;
        }
        ReviewEventKind::ReviewerAssigned { assignment } => {
            writer.write_u8(3)?;
            super::state::write_assignment(writer, assignment)?;
        }
        ReviewEventKind::ReviewSubmitted { submission } => {
            writer.write_u8(4)?;
            super::state::write_submission(writer, submission)?;
        }
        ReviewEventKind::DuplicatesReconciled { canonical, duplicates, reconciliation_digest } => {
            writer.write_u8(5)?;
            super::write_id(writer, canonical.as_bytes())?;
            write_finding_ids(writer, duplicates)?;
            super::write_digest(writer, *reconciliation_digest)?;
        }
        ReviewEventKind::FixerResponseRecorded { finding_id, response } => {
            writer.write_u8(6)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::state::write_fixer(writer, response)?;
        }
        ReviewEventKind::ResolutionConfirmed {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            writer.write_u8(7)?;
            write_confirmation(
                writer,
                *finding_id,
                None,
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
            )?;
        }
        ReviewEventKind::InvalidationConfirmed {
            finding_id,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            writer.write_u8(8)?;
            write_confirmation(
                writer,
                *finding_id,
                None,
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
            )?;
        }
        ReviewEventKind::SupersessionConfirmed {
            finding_id,
            superseding,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            writer.write_u8(9)?;
            write_confirmation(
                writer,
                *finding_id,
                Some(*superseding),
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
            )?;
        }
        ReviewEventKind::WaiverRequested { finding_id, request } => {
            writer.write_u8(10)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::state::write_fixer(writer, request)?;
        }
        ReviewEventKind::WaiverObserved { waiver } => {
            writer.write_u8(11)?;
            super::state::write_waiver(writer, *waiver)?;
        }
        ReviewEventKind::CycleCancelled { cycle_id } => {
            writer.write_u8(12)?;
            super::write_id(writer, cycle_id.as_bytes())?;
        }
        ReviewEventKind::RunCancelled => writer.write_u8(13)?,
        ReviewEventKind::BudgetExhausted { reason_digest } => {
            writer.write_u8(14)?;
            super::write_digest(writer, *reason_digest)?;
        }
        ReviewEventKind::RunFailed { failure_digest } => {
            writer.write_u8(15)?;
            super::write_digest(writer, *failure_digest)?;
        }
        ReviewEventKind::RunFinalized => writer.write_u8(16)?,
        ReviewEventKind::RunPaused => writer.write_u8(17)?,
        ReviewEventKind::RunResumed => writer.write_u8(18)?,
    }
    Ok(())
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<ReviewEventKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ReviewEventKind::RunStarted {
            binding: super::read_binding(reader)?,
            limits: super::read_limits(reader)?,
        }),
        2 => Ok(ReviewEventKind::RevisionAdvanced { binding: super::read_binding(reader)? }),
        3 => Ok(ReviewEventKind::ReviewerAssigned {
            assignment: super::state::read_assignment(reader)?,
        }),
        4 => Ok(ReviewEventKind::ReviewSubmitted {
            submission: super::state::read_submission(reader)?,
        }),
        5 => Ok(ReviewEventKind::DuplicatesReconciled {
            canonical: super::read_finding_id(reader)?,
            duplicates: read_finding_ids(reader)?,
            reconciliation_digest: super::read_digest(reader)?,
        }),
        6 => Ok(ReviewEventKind::FixerResponseRecorded {
            finding_id: super::read_finding_id(reader)?,
            response: super::state::read_fixer(reader)?,
        }),
        7 => {
            let (finding_id, _, reviewer_cycle, pending, evidence, confirmation) =
                read_confirmation(reader, false)?;
            Ok(ReviewEventKind::ResolutionConfirmed {
                finding_id,
                reviewer_cycle,
                pending_response_digest: pending,
                evidence,
                confirmation_digest: confirmation,
            })
        }
        8 => {
            let (finding_id, _, reviewer_cycle, pending, evidence, confirmation) =
                read_confirmation(reader, false)?;
            Ok(ReviewEventKind::InvalidationConfirmed {
                finding_id,
                reviewer_cycle,
                pending_response_digest: pending,
                evidence,
                confirmation_digest: confirmation,
            })
        }
        9 => {
            let (finding_id, superseding, reviewer_cycle, pending, evidence, confirmation) =
                read_confirmation(reader, true)?;
            Ok(ReviewEventKind::SupersessionConfirmed {
                finding_id,
                superseding: superseding.ok_or_else(|| super::invalid(reader))?,
                reviewer_cycle,
                pending_response_digest: pending,
                evidence,
                confirmation_digest: confirmation,
            })
        }
        10 => Ok(ReviewEventKind::WaiverRequested {
            finding_id: super::read_finding_id(reader)?,
            request: super::state::read_fixer(reader)?,
        }),
        11 => Ok(ReviewEventKind::WaiverObserved { waiver: super::state::read_waiver(reader)? }),
        12 => Ok(ReviewEventKind::CycleCancelled { cycle_id: super::read_cycle_id(reader)? }),
        13 => Ok(ReviewEventKind::RunCancelled),
        14 => Ok(ReviewEventKind::BudgetExhausted { reason_digest: super::read_digest(reader)? }),
        15 => Ok(ReviewEventKind::RunFailed { failure_digest: super::read_digest(reader)? }),
        16 => Ok(ReviewEventKind::RunFinalized),
        17 => Ok(ReviewEventKind::RunPaused),
        18 => Ok(ReviewEventKind::RunResumed),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn write_finding_ids(
    writer: &mut CanonicalWriter,
    values: &[peritus_types::FindingId],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        super::write_id(writer, value.as_bytes())?;
    }
    Ok(())
}

fn read_finding_ids(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<peritus_types::FindingId>, CodecError> {
    let count = super::bounded_len(reader, ReviewLimits::MAX_FINDINGS as usize)?;
    if count == 0 {
        return Err(super::invalid(reader));
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(super::read_finding_id(reader)?);
    }
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(super::invalid(reader));
    }
    Ok(values)
}

fn write_confirmation(
    writer: &mut CanonicalWriter,
    finding: peritus_types::FindingId,
    superseding: Option<peritus_types::FindingId>,
    cycle: peritus_types::ReviewCycleId,
    pending: peritus_types::Sha256Digest,
    evidence: &[peritus_types::EvidenceId],
    confirmation: peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    super::write_id(writer, finding.as_bytes())?;
    if let Some(superseding) = superseding {
        super::write_id(writer, superseding.as_bytes())?;
    }
    super::write_id(writer, cycle.as_bytes())?;
    super::write_digest(writer, pending)?;
    super::state::write_evidence(writer, evidence)?;
    super::write_digest(writer, confirmation)
}

type Confirmation = (
    peritus_types::FindingId,
    Option<peritus_types::FindingId>,
    peritus_types::ReviewCycleId,
    peritus_types::Sha256Digest,
    Vec<peritus_types::EvidenceId>,
    peritus_types::Sha256Digest,
);

fn read_confirmation(
    reader: &mut CanonicalReader<'_>,
    has_superseding: bool,
) -> Result<Confirmation, CodecError> {
    Ok((
        super::read_finding_id(reader)?,
        has_superseding.then(|| super::read_finding_id(reader)).transpose()?,
        super::read_cycle_id(reader)?,
        super::read_digest(reader)?,
        super::state::read_evidence(reader)?,
        super::read_digest(reader)?,
    ))
}
