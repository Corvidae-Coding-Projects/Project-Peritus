use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};

use crate::{ReviewCommand, ReviewCommandKind, ReviewLimits};

/// Canonical family-53 schema-v1 review command frame.
pub struct ReviewCommandFrame(pub ReviewCommand);

impl ReviewCommandFrame {
    /// Copies one checked review command into its canonical transport frame.
    #[must_use]
    pub fn from_command(command: &ReviewCommand) -> Self {
        Self(command.clone())
    }

    #[cfg(test)]
    /// Recovers the checked review command for canonical round-trip assertions.
    #[must_use]
    pub fn into_command(self) -> ReviewCommand {
        self.0
    }
}

impl CanonicalEncode for ReviewCommandFrame {
    const FAMILY: u16 = 53;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let command = &self.0;
        super::write_id(writer, command.command_id().as_bytes())?;
        super::write_id(writer, command.event_id().as_bytes())?;
        super::write_id(writer, command.run_id().as_bytes())?;
        writer.write_u64(command.expected_sequence())?;
        super::write_option_id(
            writer,
            command.expected_previous_event(),
            peritus_types::EventId::into_bytes,
        )?;
        super::write_digest(writer, command.prior_state_digest())?;
        super::write_revision(writer, command.revision())?;
        write_kind(writer, command.kind())
    }
}

impl CanonicalDecode for ReviewCommandFrame {
    const FAMILY: u16 = 53;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let command_id = super::read_command_id(reader)?;
        let event_id = super::read_event_id(reader)?;
        let run_id = super::read_run_id(reader)?;
        let expected_sequence = reader.read_u64()?;
        let previous =
            reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
        if (expected_sequence == 0) != previous.is_none() {
            return Err(super::invalid(reader));
        }
        ReviewCommand::new(
            command_id,
            event_id,
            run_id,
            expected_sequence,
            previous,
            super::read_digest(reader)?,
            super::read_revision(reader)?,
            read_kind(reader)?,
        )
        .map(Self)
        .map_err(|_| super::invalid(reader))
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed command tag table is kept contiguous for wire-schema review"
)]
fn write_kind(writer: &mut CanonicalWriter, kind: &ReviewCommandKind) -> Result<(), CodecError> {
    match kind {
        ReviewCommandKind::StartRun { binding, limits } => {
            writer.write_u8(1)?;
            super::write_binding(writer, binding)?;
            super::write_limits(writer, *limits)?;
        }
        ReviewCommandKind::AdvanceRevision { binding } => {
            writer.write_u8(2)?;
            super::write_binding(writer, binding)?;
        }
        ReviewCommandKind::AssignReviewer { assignment } => {
            writer.write_u8(3)?;
            super::state::write_assignment(writer, assignment)?;
        }
        ReviewCommandKind::SubmitReview { submission } => {
            writer.write_u8(4)?;
            super::state::write_submission(writer, submission)?;
        }
        ReviewCommandKind::ReconcileDuplicates { canonical, duplicates, reconciliation_digest } => {
            writer.write_u8(5)?;
            super::write_id(writer, canonical.as_bytes())?;
            write_finding_ids(writer, duplicates)?;
            super::write_digest(writer, *reconciliation_digest)?;
        }
        ReviewCommandKind::RecordFixerResponse { finding_id, response } => {
            writer.write_u8(6)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::state::write_fixer(writer, response)?;
        }
        ReviewCommandKind::ConfirmResolution {
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
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
            )?;
        }
        ReviewCommandKind::ConfirmInvalidation {
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
                *reviewer_cycle,
                *pending_response_digest,
                evidence,
                *confirmation_digest,
            )?;
        }
        ReviewCommandKind::ConfirmSupersession {
            finding_id,
            superseding,
            reviewer_cycle,
            pending_response_digest,
            evidence,
            confirmation_digest,
        } => {
            writer.write_u8(9)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::write_id(writer, superseding.as_bytes())?;
            super::write_id(writer, reviewer_cycle.as_bytes())?;
            super::write_digest(writer, *pending_response_digest)?;
            super::state::write_evidence(writer, evidence)?;
            super::write_digest(writer, *confirmation_digest)?;
        }
        ReviewCommandKind::RequestWaiver { finding_id, request } => {
            writer.write_u8(10)?;
            super::write_id(writer, finding_id.as_bytes())?;
            super::state::write_fixer(writer, request)?;
        }
        ReviewCommandKind::ObserveWaiver { waiver } => {
            writer.write_u8(11)?;
            super::state::write_waiver(writer, *waiver)?;
        }
        ReviewCommandKind::CancelCycle { cycle_id } => {
            writer.write_u8(12)?;
            super::write_id(writer, cycle_id.as_bytes())?;
        }
        ReviewCommandKind::CancelRun => writer.write_u8(13)?,
        ReviewCommandKind::ExhaustBudget { reason_digest } => {
            writer.write_u8(14)?;
            super::write_digest(writer, *reason_digest)?;
        }
        ReviewCommandKind::FailRun { failure_digest } => {
            writer.write_u8(15)?;
            super::write_digest(writer, *failure_digest)?;
        }
        ReviewCommandKind::FinalizeRun => writer.write_u8(16)?,
        ReviewCommandKind::PauseRun => writer.write_u8(17)?,
        ReviewCommandKind::ResumeRun => writer.write_u8(18)?,
    }
    Ok(())
}

fn read_kind(reader: &mut CanonicalReader<'_>) -> Result<ReviewCommandKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(ReviewCommandKind::StartRun {
            binding: super::read_binding(reader)?,
            limits: super::read_limits(reader)?,
        }),
        2 => Ok(ReviewCommandKind::AdvanceRevision { binding: super::read_binding(reader)? }),
        3 => Ok(ReviewCommandKind::AssignReviewer {
            assignment: super::state::read_assignment(reader)?,
        }),
        4 => Ok(ReviewCommandKind::SubmitReview {
            submission: super::state::read_submission(reader)?,
        }),
        5 => Ok(ReviewCommandKind::ReconcileDuplicates {
            canonical: super::read_finding_id(reader)?,
            duplicates: read_finding_ids(reader)?,
            reconciliation_digest: super::read_digest(reader)?,
        }),
        6 => Ok(ReviewCommandKind::RecordFixerResponse {
            finding_id: super::read_finding_id(reader)?,
            response: super::state::read_fixer(reader)?,
        }),
        7 => {
            let (finding_id, reviewer_cycle, pending, evidence, confirmation) =
                read_confirmation(reader)?;
            Ok(ReviewCommandKind::ConfirmResolution {
                finding_id,
                reviewer_cycle,
                pending_response_digest: pending,
                evidence,
                confirmation_digest: confirmation,
            })
        }
        8 => {
            let (finding_id, reviewer_cycle, pending, evidence, confirmation) =
                read_confirmation(reader)?;
            Ok(ReviewCommandKind::ConfirmInvalidation {
                finding_id,
                reviewer_cycle,
                pending_response_digest: pending,
                evidence,
                confirmation_digest: confirmation,
            })
        }
        9 => Ok(ReviewCommandKind::ConfirmSupersession {
            finding_id: super::read_finding_id(reader)?,
            superseding: super::read_finding_id(reader)?,
            reviewer_cycle: super::read_cycle_id(reader)?,
            pending_response_digest: super::read_digest(reader)?,
            evidence: super::state::read_evidence(reader)?,
            confirmation_digest: super::read_digest(reader)?,
        }),
        10 => Ok(ReviewCommandKind::RequestWaiver {
            finding_id: super::read_finding_id(reader)?,
            request: super::state::read_fixer(reader)?,
        }),
        11 => Ok(ReviewCommandKind::ObserveWaiver { waiver: super::state::read_waiver(reader)? }),
        12 => Ok(ReviewCommandKind::CancelCycle { cycle_id: super::read_cycle_id(reader)? }),
        13 => Ok(ReviewCommandKind::CancelRun),
        14 => Ok(ReviewCommandKind::ExhaustBudget { reason_digest: super::read_digest(reader)? }),
        15 => Ok(ReviewCommandKind::FailRun { failure_digest: super::read_digest(reader)? }),
        16 => Ok(ReviewCommandKind::FinalizeRun),
        17 => Ok(ReviewCommandKind::PauseRun),
        18 => Ok(ReviewCommandKind::ResumeRun),
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
    cycle: peritus_types::ReviewCycleId,
    pending: peritus_types::Sha256Digest,
    evidence: &[peritus_types::EvidenceId],
    confirmation: peritus_types::Sha256Digest,
) -> Result<(), CodecError> {
    super::write_id(writer, finding.as_bytes())?;
    super::write_id(writer, cycle.as_bytes())?;
    super::write_digest(writer, pending)?;
    super::state::write_evidence(writer, evidence)?;
    super::write_digest(writer, confirmation)
}

type Confirmation = (
    peritus_types::FindingId,
    peritus_types::ReviewCycleId,
    peritus_types::Sha256Digest,
    Vec<peritus_types::EvidenceId>,
    peritus_types::Sha256Digest,
);

fn read_confirmation(reader: &mut CanonicalReader<'_>) -> Result<Confirmation, CodecError> {
    Ok((
        super::read_finding_id(reader)?,
        super::read_cycle_id(reader)?,
        super::read_digest(reader)?,
        super::state::read_evidence(reader)?,
        super::read_digest(reader)?,
    ))
}

#[cfg(test)]
mod codec_tests {
    use peritus_codec::{CodecLimits, decode_message, encode_message};
    use peritus_types::{
        AcceptanceSpecId, CommandId, EventId, EventSequence, Generation, HarnessId, PolicyId,
        ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
    };

    use super::ReviewCommandFrame;
    use crate::wire::ReviewEventFrame;
    use crate::{ReviewCommand, ReviewCommandKind, ReviewEvent, ReviewEventKind};

    #[test]
    fn codec_command_and_event_round_trip_with_closed_tags() {
        let revision = revision();
        let previous = EventId::new(bytes(2)).unwrap();
        let command = ReviewCommand::new(
            CommandId::new(bytes(3)).unwrap(),
            EventId::new(bytes(4)).unwrap(),
            RunId::new(bytes(5)).unwrap(),
            1,
            Some(previous),
            digest(6),
            revision,
            ReviewCommandKind::CancelRun,
        )
        .unwrap();
        let encoded =
            encode_message(&ReviewCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
                .unwrap();
        let decoded = decode_message::<ReviewCommandFrame>(&encoded, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command();
        assert_eq!(decoded, command);

        let event = ReviewEvent::from_wire(
            command.event_id(),
            command.command_id(),
            EventSequence::new(2).unwrap(),
            Some(previous),
            command.run_id(),
            revision,
            digest(6),
            digest(7),
            ReviewEventKind::RunCancelled,
        );
        let event_bytes =
            encode_message(&ReviewEventFrame(event.clone()), CodecLimits::PRODUCTION).unwrap();
        assert_eq!(
            decode_message::<ReviewEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
                .unwrap()
                .into_event(),
            event
        );

        let mut unknown = encoded;
        *unknown.last_mut().unwrap() = u8::MAX;
        assert!(decode_message::<ReviewCommandFrame>(&unknown, CodecLimits::PRODUCTION).is_err());
    }

    fn revision() -> RevisionTuple {
        RevisionTuple::new(
            AcceptanceSpecId::new(bytes(10)).unwrap(),
            HarnessId::new(bytes(11)).unwrap(),
            WorkspaceId::new(bytes(12)).unwrap(),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new(bytes(13)).unwrap(),
            ProviderProfileId::new(bytes(14)).unwrap(),
        )
    }

    const fn bytes(value: u8) -> [u8; 16] {
        [value; 16]
    }
    const fn digest(value: u8) -> Sha256Digest {
        Sha256Digest::new([value; 32])
    }
}
