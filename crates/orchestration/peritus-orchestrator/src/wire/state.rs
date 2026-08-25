//! Canonical family-78 complete E0 checkpoint codec.

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecLimits,
};
use peritus_types::Sha256Digest;

use crate::{OrchestratorCounters, OrchestratorState};

/// Canonical family-78 schema-v1 complete orchestrator checkpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorStateFrame(OrchestratorState);

impl OrchestratorStateFrame {
    /// Copies a validated checkpoint into its canonical transport frame.
    #[must_use]
    pub fn from_state(state: &OrchestratorState) -> Self {
        Self(state.clone())
    }
    /// Returns whether this frame contains the exact supplied semantic state.
    #[must_use]
    pub fn matches_state(&self, state: &OrchestratorState) -> bool {
        &self.0 == state
    }
    /// Returns the framed E0 run identity.
    #[must_use]
    pub const fn run_id(&self) -> peritus_types::RunId {
        self.0.binding().run_id()
    }
    /// Returns the framed checkpoint event sequence.
    #[must_use]
    pub const fn sequence(&self) -> peritus_types::EventSequence {
        self.0.sequence()
    }
    /// Returns the framed aggregate head event identity.
    #[must_use]
    pub const fn last_event_id(&self) -> peritus_types::EventId {
        self.0.last_event_id()
    }
    /// Returns the framed current candidate revision.
    #[must_use]
    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.0.current_candidate().revision()
    }
    /// Returns the framed complete semantic state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.0.state_digest()
    }
    /// Consumes the frame and returns its validated checkpoint.
    #[must_use]
    pub fn into_state(self) -> OrchestratorState {
        self.0
    }
}

impl CanonicalEncode for OrchestratorStateFrame {
    const FAMILY: u16 = 78;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        write_state(writer, &self.0, true)
    }
}

impl CanonicalDecode for OrchestratorStateFrame {
    const FAMILY: u16 = 78;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let binding = crate::canonical::wire::domain::read_binding(reader)?;
        let limits = binding.limits();
        let ownership = crate::canonical::wire::domain::read_ownership(reader, limits)?;
        let phase = crate::canonical::wire::read_phase(reader)?;
        let sequence = crate::canonical::wire::read_sequence(reader)?;
        let last_event = crate::canonical::wire::read_event_id(reader)?;
        let state_digest = crate::canonical::wire::read_digest(reader)?;
        let current_candidate = crate::canonical::wire::domain::read_candidate(reader, limits)?;
        let candidate_history = read_candidates(reader, limits)?;
        let current_quality_cycle = crate::canonical::wire::domain::read_quality_cycle(reader)?;
        let quality_cycle_history = read_quality_cycles(reader, limits)?;
        let proposed_candidate = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_candidate(reader, limits))
            .transpose()?;
        let counters = read_counters(reader)?;
        let handoffs = read_handoffs(reader, limits)?;
        let open_handoff = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_handoff(reader, limits))
            .transpose()?;
        let activations = read_activations(reader, limits)?;
        let observations = read_observations(reader, limits)?;
        let active_children = read_child_kinds(reader)?;
        let pending_directive = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_directive(reader))
            .transpose()?;
        let certificate = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_certificate(reader))
            .transpose()?;
        let cancellation_cause = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::read_digest(reader))
            .transpose()?;
        let used_commands = read_commands(reader)?;
        let terminal = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_terminal(reader))
            .transpose()?;
        let pending_terminal = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_terminal(reader))
            .transpose()?;
        let paused_reconciliation = reader
            .read_option_tag()?
            .then(|| crate::canonical::wire::domain::read_reconciliation(reader))
            .transpose()?;
        let paused_children = read_child_kinds(reader)?;
        let state = OrchestratorState::from_wire(
            binding,
            ownership,
            phase,
            sequence,
            last_event,
            state_digest,
            current_candidate,
            candidate_history,
            current_quality_cycle,
            quality_cycle_history,
            proposed_candidate,
            counters,
            handoffs,
            open_handoff,
            activations,
            observations,
            active_children,
            pending_directive,
            certificate,
            cancellation_cause,
            used_commands,
            terminal,
            pending_terminal,
            paused_reconciliation,
            paused_children,
        );
        state.validate().map_err(|_| crate::canonical::wire::invalid(reader))?;
        Ok(Self(state))
    }
}

pub fn canonical_state_bytes(state: &OrchestratorState) -> Result<Vec<u8>, CodecError> {
    let mut writer = CanonicalWriter::new(CodecLimits::new(
        usize::MAX,
        usize::MAX,
        u32::MAX as usize,
        usize::MAX,
        usize::MAX,
        u16::MAX,
    ));
    write_state(&mut writer, state, false)?;
    Ok(writer.into_bytes())
}

fn write_state(
    writer: &mut CanonicalWriter,
    state: &OrchestratorState,
    stored_digest: bool,
) -> Result<(), CodecError> {
    crate::canonical::wire::domain::write_binding(writer, state.binding())?;
    crate::canonical::wire::domain::write_ownership(writer, state.ownership())?;
    crate::canonical::wire::write_phase(writer, state.phase())?;
    writer.write_u64(state.sequence().get())?;
    crate::canonical::wire::write_id(writer, state.last_event_id().as_bytes())?;
    crate::canonical::wire::write_digest(
        writer,
        if stored_digest { state.state_digest() } else { Sha256Digest::new([0; 32]) },
    )?;
    crate::canonical::wire::domain::write_candidate(writer, state.current_candidate())?;
    write_candidates(writer, state.candidate_history())?;
    crate::canonical::wire::domain::write_quality_cycle(writer, state.current_quality_cycle())?;
    write_quality_cycles(writer, state.quality_cycle_history())?;
    writer.write_option_tag(state.proposed_candidate().is_some())?;
    if let Some(value) = state.proposed_candidate() {
        crate::canonical::wire::domain::write_candidate(writer, value)?;
    }
    write_counters(writer, state.counters())?;
    write_handoffs(writer, state.handoffs())?;
    writer.write_option_tag(state.open_handoff().is_some())?;
    if let Some(value) = state.open_handoff() {
        crate::canonical::wire::domain::write_handoff(writer, value)?;
    }
    writer.write_collection_len(state.activations().len())?;
    for value in state.activations() {
        crate::canonical::wire::observation::write_activation(writer, value)?;
    }
    writer.write_collection_len(state.children().len())?;
    for value in state.children() {
        crate::canonical::wire::observation::write_observation(writer, value)?;
    }
    write_child_kinds(writer, state.active_children())?;
    writer.write_option_tag(state.pending_directive().is_some())?;
    if let Some(value) = state.pending_directive() {
        crate::canonical::wire::domain::write_directive(writer, value)?;
    }
    writer.write_option_tag(state.acceptance_certificate().is_some())?;
    if let Some(value) = state.acceptance_certificate() {
        crate::canonical::wire::domain::write_certificate(writer, value)?;
    }
    writer.write_option_tag(state.cancellation_cause().is_some())?;
    if let Some(value) = state.cancellation_cause() {
        crate::canonical::wire::write_digest(writer, value)?;
    }
    writer.write_collection_len(state.used_commands().len())?;
    for command in state.used_commands() {
        crate::canonical::wire::write_id(writer, command.as_bytes())?;
    }
    write_terminal_option(writer, state.terminal().copied())?;
    write_terminal_option(writer, state.pending_terminal().copied())?;
    writer.write_option_tag(state.paused_reconciliation().is_some())?;
    if let Some(value) = state.paused_reconciliation() {
        crate::canonical::wire::domain::write_reconciliation(writer, value)?;
    }
    write_child_kinds(writer, state.paused_children())
}

fn write_candidates(
    writer: &mut CanonicalWriter,
    values: &[crate::CandidateBinding],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        crate::canonical::wire::domain::write_candidate(writer, value)?;
    }
    Ok(())
}
fn read_candidates(
    reader: &mut CanonicalReader<'_>,
    limits: crate::OrchestratorLimits,
) -> Result<Vec<crate::CandidateBinding>, CodecError> {
    let count = bounded(reader, usize::from(limits.revisions()))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::domain::read_candidate(reader, limits)?);
    }
    Ok(values)
}
fn write_quality_cycles(
    writer: &mut CanonicalWriter,
    values: &[crate::QualityCycleBinding],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        crate::canonical::wire::domain::write_quality_cycle(writer, value)?;
    }
    Ok(())
}
fn read_quality_cycles(
    reader: &mut CanonicalReader<'_>,
    limits: crate::OrchestratorLimits,
) -> Result<Vec<crate::QualityCycleBinding>, CodecError> {
    let count = bounded(reader, usize::from(limits.revisions()))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::domain::read_quality_cycle(reader)?);
    }
    Ok(values)
}
fn write_handoffs(
    writer: &mut CanonicalWriter,
    values: &[crate::Handoff],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        crate::canonical::wire::domain::write_handoff(writer, value)?;
    }
    Ok(())
}
fn read_handoffs(
    reader: &mut CanonicalReader<'_>,
    limits: crate::OrchestratorLimits,
) -> Result<Vec<crate::Handoff>, CodecError> {
    let count = bounded(reader, usize::from(limits.handoffs()))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::domain::read_handoff(reader, limits)?);
    }
    Ok(values)
}
fn read_activations(
    reader: &mut CanonicalReader<'_>,
    limits: crate::OrchestratorLimits,
) -> Result<Vec<crate::HandoffActivationObservation>, CodecError> {
    let count = bounded(reader, usize::from(limits.retained_observations()))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::observation::read_activation(reader)?);
    }
    Ok(values)
}
fn read_observations(
    reader: &mut CanonicalReader<'_>,
    limits: crate::OrchestratorLimits,
) -> Result<Vec<crate::ChildObservation>, CodecError> {
    let count = bounded(reader, usize::from(limits.retained_observations()))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::observation::read_observation(reader)?);
    }
    Ok(values)
}
fn write_child_kinds(
    writer: &mut CanonicalWriter,
    values: &[crate::ChildAggregateKind],
) -> Result<(), CodecError> {
    writer.write_collection_len(values.len())?;
    for value in values {
        writer.write_u8(crate::canonical::wire::child_kind_tag(*value))?;
    }
    Ok(())
}
fn read_child_kinds(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<crate::ChildAggregateKind>, CodecError> {
    let count = bounded(reader, 6)?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::read_child_kind(reader)?);
    }
    Ok(values)
}
fn write_counters(
    writer: &mut CanonicalWriter,
    value: OrchestratorCounters,
) -> Result<(), CodecError> {
    for item in [
        value.revisions(),
        value.writer_cycles(),
        value.fixer_cycles(),
        value.gate_cycles(),
        value.review_cycles(),
        value.handoffs(),
        value.child_directives(),
        value.retained_observations(),
        value.cancellation_reconciliations(),
    ] {
        writer.write_u16(item)?;
    }
    Ok(())
}
fn read_counters(reader: &mut CanonicalReader<'_>) -> Result<OrchestratorCounters, CodecError> {
    Ok(OrchestratorCounters::from_wire(
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
        reader.read_u16()?,
    ))
}
fn read_commands(
    reader: &mut CanonicalReader<'_>,
) -> Result<Vec<peritus_types::CommandId>, CodecError> {
    let count = bounded(reader, usize::from(u16::MAX))?;
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(crate::canonical::wire::read_command_id(reader)?);
    }
    Ok(values)
}
fn write_terminal_option(
    writer: &mut CanonicalWriter,
    value: Option<crate::OrchestratorTerminal>,
) -> Result<(), CodecError> {
    writer.write_option_tag(value.is_some())?;
    if let Some(value) = value {
        crate::canonical::wire::domain::write_terminal(writer, value)?;
    }
    Ok(())
}
fn bounded(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let count = reader.read_collection_len()?;
    if count <= maximum { Ok(count) } else { Err(crate::canonical::wire::invalid(reader)) }
}
