mod disposition;
mod finding;
mod summary;

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_context::ContextPlanId;
use peritus_quality_policy::ReviewCycleOrdinal;
use peritus_spec::ReviewCategory;
use peritus_types::EventSequence;

use crate::{
    ReviewAssignment, ReviewCycle, ReviewCyclePhase, ReviewLimits, ReviewRunPhase, ReviewRunState,
    ReviewSubmission,
};

pub(super) use disposition::{
    read_evidence, read_fixer, read_waiver, write_evidence, write_fixer, write_waiver,
};

/// Canonical family-55 schema-v1 complete review-state checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewStateFrame(ReviewRunState);

impl ReviewStateFrame {
    pub fn from_state(state: &ReviewRunState) -> Self {
        Self(state.clone())
    }

    pub fn matches_state(&self, state: &ReviewRunState) -> bool {
        &self.0 == state
    }

    pub const fn run_id(&self) -> peritus_types::RunId {
        self.0.run_id()
    }

    pub const fn sequence(&self) -> EventSequence {
        self.0.sequence()
    }

    pub const fn last_event_id(&self) -> peritus_types::EventId {
        self.0.last_event_id()
    }

    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.0.binding().revision()
    }

    pub const fn state_digest(&self) -> peritus_types::Sha256Digest {
        self.0.state_digest()
    }
}

impl CanonicalEncode for ReviewStateFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        let state = &self.0;
        super::write_id(writer, state.run_id().as_bytes())?;
        super::write_limits(writer, state.limits())?;
        super::write_binding(writer, state.binding())?;
        writer.write_u8(crate::canonical::run_phase_tag(state.phase()))?;
        writer.write_u64(state.sequence().get())?;
        super::write_id(writer, state.last_event_id().as_bytes())?;
        super::write_digest(writer, state.state_digest())?;
        writer.write_collection_len(state.cycles().len())?;
        for cycle in state.cycles() {
            write_cycle(writer, cycle)?;
        }
        writer.write_collection_len(state.findings().len())?;
        for finding in state.findings() {
            finding::write_finding(writer, finding)?;
        }
        writer.write_collection_len(state.waivers().len())?;
        for waiver in state.waivers() {
            write_waiver(writer, *waiver)?;
        }
        summary::write_quorum(writer, state.quorum())?;
        summary::write_oscillation(writer, state.oscillation())?;
        writer.write_collection_len(state.used_commands().len())?;
        for command in state.used_commands() {
            super::write_id(writer, command.as_bytes())?;
        }
        writer.write_option_tag(state.terminal().is_some())?;
        if let Some(terminal) = state.terminal() {
            summary::write_terminal(writer, terminal)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for ReviewStateFrame {
    const FAMILY: u16 = 55;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let run_id = super::read_run_id(reader)?;
        let limits = super::read_limits(reader)?;
        let binding = super::read_binding(reader)?;
        let phase_offset = reader.offset();
        let phase = match reader.read_u8()? {
            1 => ReviewRunPhase::Active,
            2 => ReviewRunPhase::Terminal,
            _ => return Err(super::unknown(phase_offset)),
        };
        let sequence_offset = reader.offset();
        let sequence = EventSequence::new(reader.read_u64()?)
            .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, sequence_offset))?;
        let last_event_id = super::read_event_id(reader)?;
        let state_digest = super::read_digest(reader)?;
        let cycle_count =
            super::bounded_len(reader, usize::from(limits.cycles().min(limits.assignments())))?;
        let mut cycles = Vec::with_capacity(cycle_count);
        for _ in 0..cycle_count {
            cycles.push(read_cycle(reader)?);
        }
        let finding_count = super::bounded_len(reader, limits.findings() as usize)?;
        let mut findings = Vec::with_capacity(finding_count);
        for _ in 0..finding_count {
            findings.push(finding::read_finding(reader)?);
        }
        let waiver_count = super::bounded_len(reader, limits.findings() as usize)?;
        let mut waivers = Vec::with_capacity(waiver_count);
        for _ in 0..waiver_count {
            waivers.push(read_waiver(reader)?);
        }
        let quorum = summary::read_quorum(reader)?;
        let oscillation = summary::read_oscillation(reader)?;
        let command_count = reader.read_collection_len()?;
        let mut used_commands = Vec::with_capacity(command_count);
        for _ in 0..command_count {
            used_commands.push(super::read_command_id(reader)?);
        }
        let terminal =
            reader.read_option_tag()?.then(|| summary::read_terminal(reader)).transpose()?;
        let state = ReviewRunState::from_wire(
            run_id,
            limits,
            binding,
            phase,
            sequence,
            last_event_id,
            state_digest,
            cycles,
            findings,
            waivers,
            quorum,
            oscillation,
            used_commands,
            terminal,
        );
        state.validate_inert().map_err(|_| super::invalid(reader))?;
        Ok(Self(state))
    }
}

pub(super) fn write_assignment(
    writer: &mut CanonicalWriter,
    value: &ReviewAssignment,
) -> Result<(), CodecError> {
    super::write_id(writer, value.cycle_id().as_bytes())?;
    writer.write_u16(value.ordinal().get())?;
    super::write_digest(writer, value.binding_digest())?;
    super::write_revision(writer, value.revision())?;
    super::write_reviewer(writer, value.reviewer())?;
    writer.write_collection_len(value.categories().len())?;
    for category in value.categories() {
        super::write_digest(writer, category.digest())?;
    }
    super::write_digest(writer, value.context_plan_id().digest())?;
    writer.write_bool(value.fresh_context())?;
    super::write_independence(writer, value.independence())
}

pub(super) fn read_assignment(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewAssignment, CodecError> {
    let cycle = super::read_cycle_id(reader)?;
    let ordinal_offset = reader.offset();
    let ordinal = ReviewCycleOrdinal::new(reader.read_u16()?)
        .map_err(|_| CodecError::at(CodecErrorKind::InvalidDomainValue, ordinal_offset))?;
    Ok(ReviewAssignment::from_wire(
        cycle,
        ordinal,
        super::read_digest(reader)?,
        super::read_revision(reader)?,
        super::read_reviewer(reader)?,
        super::read_digests(reader, ReviewLimits::MAX_CATEGORIES, ReviewCategory::new)?,
        ContextPlanId::new(super::read_digest(reader)?),
        reader.read_bool()?,
        super::read_independence(reader)?,
    ))
}

fn write_cycle(writer: &mut CanonicalWriter, value: &ReviewCycle) -> Result<(), CodecError> {
    write_assignment(writer, value.assignment())?;
    writer.write_u8(crate::canonical::cycle_phase_tag(value.phase()))?;
    writer.write_option_tag(value.submission().is_some())?;
    if let Some(submission) = value.submission() {
        write_submission(writer, submission)?;
    }
    Ok(())
}

fn read_cycle(reader: &mut CanonicalReader<'_>) -> Result<ReviewCycle, CodecError> {
    let assignment = read_assignment(reader)?;
    let offset = reader.offset();
    let phase = match reader.read_u8()? {
        1 => ReviewCyclePhase::Assigned,
        2 => ReviewCyclePhase::Submitted,
        3 => ReviewCyclePhase::Cancelled,
        4 => ReviewCyclePhase::Invalidated,
        _ => return Err(super::unknown(offset)),
    };
    let submission = reader.read_option_tag()?.then(|| read_submission(reader)).transpose()?;
    Ok(ReviewCycle::from_wire(assignment, phase, submission))
}

pub(super) fn write_submission(
    writer: &mut CanonicalWriter,
    value: &ReviewSubmission,
) -> Result<(), CodecError> {
    super::write_id(writer, value.cycle_id().as_bytes())?;
    super::write_revision(writer, value.revision())?;
    writer.write_collection_len(value.categories().len())?;
    for category in value.categories() {
        super::write_digest(writer, category.digest())?;
    }
    writer.write_collection_len(value.findings().len())?;
    for finding in value.findings() {
        finding::write_finding(writer, finding)?;
    }
    super::write_digest(writer, value.review_digest())
}

pub(super) fn read_submission(
    reader: &mut CanonicalReader<'_>,
) -> Result<ReviewSubmission, CodecError> {
    let cycle = super::read_cycle_id(reader)?;
    let revision = super::read_revision(reader)?;
    let categories =
        super::read_digests(reader, ReviewLimits::MAX_CATEGORIES, ReviewCategory::new)?;
    let count = super::bounded_len(reader, ReviewLimits::MAX_FINDINGS as usize)?;
    let mut findings = Vec::with_capacity(count);
    for _ in 0..count {
        findings.push(finding::read_finding(reader)?);
    }
    Ok(ReviewSubmission::from_wire(
        cycle,
        revision,
        categories,
        findings,
        super::read_digest(reader)?,
    ))
}
