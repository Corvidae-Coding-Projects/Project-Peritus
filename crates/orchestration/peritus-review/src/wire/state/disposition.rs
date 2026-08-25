//! Canonical finding-disposition, fixer-response, evidence, and waiver wire encoding.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_quality_policy::WaiverObservation;
use peritus_spec::{ContentReference, EvidenceRequirementId};
use peritus_types::FindingId;

use crate::{DispositionKind, DispositionRecord, FixerResponse, ObservedWaiver, ReviewLimits};

pub(super) fn write_disposition(
    writer: &mut CanonicalWriter,
    value: &DispositionRecord,
) -> Result<(), CodecError> {
    super::super::write_id(writer, value.event_id().as_bytes())?;
    writer.write_u8(crate::canonical::disposition_tag(value.kind()))?;
    super::super::write_option_id(writer, value.actor(), peritus_types::ActorId::into_bytes)?;
    super::super::write_option_id(
        writer,
        value.reviewer_cycle(),
        peritus_types::ReviewCycleId::into_bytes,
    )?;
    super::super::write_revision(writer, value.revision())?;
    write_evidence(writer, value.evidence())?;
    super::super::write_option_id(writer, value.related_finding(), FindingId::into_bytes)?;
    super::super::write_option_id(
        writer,
        value.approval_request_id(),
        peritus_types::ApprovalRequestId::into_bytes,
    )?;
    writer.write_option_tag(value.authority().is_some())?;
    if let Some(authority) = value.authority() {
        super::super::write_digest(writer, authority.digest())?;
    }
    writer.write_option_tag(value.evidence_requirement_id().is_some())?;
    if let Some(requirement) = value.evidence_requirement_id() {
        super::super::write_digest(writer, requirement.digest())?;
    }
    super::super::write_digest(writer, value.record_digest())
}

pub(super) fn read_disposition(
    reader: &mut CanonicalReader<'_>,
) -> Result<DispositionRecord, CodecError> {
    let event = super::super::read_event_id(reader)?;
    let offset = reader.offset();
    let kind = match reader.read_u8()? {
        1 => DispositionKind::Open,
        2 => DispositionKind::Fixed,
        3 => DispositionKind::Disputed,
        4 => DispositionKind::SupersessionProposed,
        5 => DispositionKind::WaiverRequested,
        6 => DispositionKind::ResolutionConfirmed,
        7 => DispositionKind::InvalidationConfirmed,
        8 => DispositionKind::Superseded,
        9 => DispositionKind::Waived,
        _ => return Err(super::super::unknown(offset)),
    };
    Ok(DispositionRecord::from_wire(
        event,
        kind,
        reader.read_option_tag()?.then(|| super::super::read_actor_id(reader)).transpose()?,
        reader.read_option_tag()?.then(|| super::super::read_cycle_id(reader)).transpose()?,
        super::super::read_revision(reader)?,
        read_evidence(reader)?,
        reader.read_option_tag()?.then(|| super::super::read_finding_id(reader)).transpose()?,
        reader.read_option_tag()?.then(|| super::super::read_approval_id(reader)).transpose()?,
        reader
            .read_option_tag()?
            .then(|| super::super::read_digest(reader).map(ContentReference::new))
            .transpose()?,
        reader
            .read_option_tag()?
            .then(|| super::super::read_digest(reader).map(EvidenceRequirementId::new))
            .transpose()?,
        super::super::read_digest(reader)?,
    ))
}

pub(in crate::wire) fn write_fixer(
    writer: &mut CanonicalWriter,
    value: &FixerResponse,
) -> Result<(), CodecError> {
    match value {
        FixerResponse::Fixed { fixer, revision, evidence, response_digest } => {
            writer.write_u8(1)?;
            super::super::write_id(writer, fixer.as_bytes())?;
            super::super::write_revision(writer, *revision)?;
            write_evidence(writer, evidence)?;
            super::super::write_digest(writer, *response_digest)
        }
        FixerResponse::Disputed { fixer, revision, evidence, response_digest } => {
            writer.write_u8(2)?;
            super::super::write_id(writer, fixer.as_bytes())?;
            super::super::write_revision(writer, *revision)?;
            write_evidence(writer, evidence)?;
            super::super::write_digest(writer, *response_digest)
        }
        FixerResponse::SupersessionProposed {
            fixer,
            revision,
            superseding,
            evidence,
            response_digest,
        } => {
            writer.write_u8(3)?;
            super::super::write_id(writer, fixer.as_bytes())?;
            super::super::write_revision(writer, *revision)?;
            super::super::write_id(writer, superseding.as_bytes())?;
            write_evidence(writer, evidence)?;
            super::super::write_digest(writer, *response_digest)
        }
        FixerResponse::WaiverRequested {
            requester,
            revision,
            approval_request_id,
            authority,
            evidence_requirement_id,
            request_digest,
        } => {
            writer.write_u8(4)?;
            super::super::write_id(writer, requester.as_bytes())?;
            super::super::write_revision(writer, *revision)?;
            super::super::write_id(writer, approval_request_id.as_bytes())?;
            super::super::write_digest(writer, authority.digest())?;
            super::super::write_digest(writer, evidence_requirement_id.digest())?;
            super::super::write_digest(writer, *request_digest)
        }
    }
}

pub(in crate::wire) fn read_fixer(
    reader: &mut CanonicalReader<'_>,
) -> Result<FixerResponse, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => FixerResponse::fixed(
            super::super::read_actor_id(reader)?,
            super::super::read_revision(reader)?,
            read_evidence(reader)?,
            super::super::read_digest(reader)?,
            super::super::production_limits(),
        ),
        2 => FixerResponse::disputed(
            super::super::read_actor_id(reader)?,
            super::super::read_revision(reader)?,
            read_evidence(reader)?,
            super::super::read_digest(reader)?,
            super::super::production_limits(),
        ),
        3 => FixerResponse::supersession_proposed(
            super::super::read_actor_id(reader)?,
            super::super::read_revision(reader)?,
            super::super::read_finding_id(reader)?,
            read_evidence(reader)?,
            super::super::read_digest(reader)?,
            super::super::production_limits(),
        ),
        4 => Ok(FixerResponse::waiver_requested(
            super::super::read_actor_id(reader)?,
            super::super::read_revision(reader)?,
            super::super::read_approval_id(reader)?,
            ContentReference::new(super::super::read_digest(reader)?),
            EvidenceRequirementId::new(super::super::read_digest(reader)?),
            super::super::read_digest(reader)?,
        )),
        _ => return Err(super::super::unknown(offset)),
    }
    .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
}

pub(in crate::wire) fn write_evidence(
    writer: &mut CanonicalWriter,
    values: &[peritus_types::EvidenceId],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        super::super::write_id(writer, value.as_bytes())?;
    }
    Ok(())
}

pub(in crate::wire) fn read_evidence(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<peritus_types::EvidenceId>, CodecError> {
    let count =
        super::super::bounded_len(reader, usize::from(ReviewLimits::MAX_EVIDENCE_REFERENCES))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(super::super::read_evidence_id(reader)?);
    }
    Ok(values)
}

pub(in crate::wire) fn write_waiver(
    writer: &mut CanonicalWriter,
    value: ObservedWaiver,
) -> Result<(), CodecError> {
    let observation = value.observation();
    super::super::write_id(writer, observation.finding_id().as_bytes())?;
    super::super::write_revision(writer, observation.revision())?;
    super::super::write_id(writer, observation.approval_request_id().as_bytes())?;
    super::super::write_digest(writer, observation.authority().digest())?;
    super::super::write_digest(writer, observation.evidence_requirement_id().digest())?;
    super::super::write_digest(writer, observation.waiver_digest())?;
    super::super::write_digest(writer, value.request_digest())
}

pub(in crate::wire) fn read_waiver(
    reader: &mut CanonicalReader<'_>,
) -> Result<ObservedWaiver, CodecError> {
    let finding = super::super::read_finding_id(reader)?;
    let revision = super::super::read_revision(reader)?;
    let request = super::super::read_approval_id(reader)?;
    let authority = ContentReference::new(super::super::read_digest(reader)?);
    let evidence = EvidenceRequirementId::new(super::super::read_digest(reader)?);
    let waiver_digest = super::super::read_digest(reader)?;
    Ok(ObservedWaiver::from_wire(
        WaiverObservation::new(finding, revision, request, authority, evidence, waiver_digest),
        super::super::read_digest(reader)?,
    ))
}
