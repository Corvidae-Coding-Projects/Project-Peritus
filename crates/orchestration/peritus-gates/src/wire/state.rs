use std::collections::BTreeSet;

use peritus_codec::{
    CanonicalDecode, CanonicalEncode, CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind,
};
use peritus_types::{ActionId, EventSequence, GateExecutionId, GateId};

use crate::{
    ActiveAttempt, GateAttemptResult, GateEvidenceReceipt, GateRunPhase, GateRunState,
    GateSlotPhase, GateTerminalKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckpointSlot {
    gate_id: GateId,
    phase: GateSlotPhase,
    attempts: u16,
    active: Option<ActiveAttempt>,
    result: Option<GateAttemptResult>,
    result_event: Option<peritus_types::EventId>,
    evidence: Option<GateEvidenceReceipt>,
    blocked_by: Option<GateId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckpointTerminal {
    kind: GateTerminalKind,
    non_passing: Vec<GateId>,
    digest: peritus_types::Sha256Digest,
}

/// Complete typed C0 checkpoint frame. Its semantic authority remains event replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateStateFrame {
    run_id: peritus_types::RunId,
    plan_digest: peritus_types::Sha256Digest,
    revision: peritus_types::RevisionTuple,
    snapshot_digest: peritus_types::Sha256Digest,
    maximum_attempts: u16,
    phase: GateRunPhase,
    sequence: EventSequence,
    last_event_id: peritus_types::EventId,
    state_digest: peritus_types::Sha256Digest,
    slots: Vec<CheckpointSlot>,
    used_executions: Vec<GateExecutionId>,
    used_actions: Vec<ActionId>,
    terminal: Option<CheckpointTerminal>,
}

impl GateStateFrame {
    pub fn from_state(state: &GateRunState) -> Self {
        Self {
            run_id: state.run_id(),
            plan_digest: state.plan_digest(),
            revision: state.revision(),
            snapshot_digest: state.snapshot_digest(),
            maximum_attempts: state.maximum_attempts(),
            phase: state.phase(),
            sequence: state.sequence(),
            last_event_id: state.last_event_id(),
            state_digest: state.state_digest(),
            slots: state
                .slots()
                .iter()
                .map(|slot| CheckpointSlot {
                    gate_id: slot.gate_id(),
                    phase: slot.phase(),
                    attempts: slot.attempts(),
                    active: slot.active_attempt(),
                    result: slot.last_result().cloned(),
                    result_event: slot.result_event(),
                    evidence: slot.evidence().cloned(),
                    blocked_by: slot.blocked_by(),
                })
                .collect(),
            used_executions: state.used_executions().to_vec(),
            used_actions: state.used_actions().to_vec(),
            terminal: state.terminal().map(|terminal| CheckpointTerminal {
                kind: terminal.kind(),
                non_passing: terminal.non_passing().to_vec(),
                digest: terminal.digest(),
            }),
        }
    }

    pub fn matches_state(&self, state: &GateRunState) -> bool {
        self == &Self::from_state(state)
    }

    pub(crate) fn into_state(self) -> GateRunState {
        let slots = self
            .slots
            .into_iter()
            .map(|slot| {
                crate::GateSlot::from_checkpoint(
                    slot.gate_id,
                    slot.phase,
                    slot.attempts,
                    slot.active,
                    slot.result,
                    slot.result_event,
                    slot.evidence,
                    slot.blocked_by,
                )
            })
            .collect();
        let terminal = self.terminal.map(|terminal| {
            crate::GateTerminal::from_checkpoint(
                terminal.kind,
                terminal.non_passing,
                terminal.digest,
            )
        });
        GateRunState::from_checkpoint(
            self.run_id,
            self.plan_digest,
            self.revision,
            self.snapshot_digest,
            self.maximum_attempts,
            self.phase,
            self.sequence,
            self.last_event_id,
            self.state_digest,
            slots,
            self.used_executions,
            self.used_actions,
            terminal,
        )
    }

    pub const fn run_id(&self) -> peritus_types::RunId {
        self.run_id
    }

    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }

    pub const fn last_event_id(&self) -> peritus_types::EventId {
        self.last_event_id
    }

    pub const fn revision(&self) -> peritus_types::RevisionTuple {
        self.revision
    }

    pub const fn state_digest(&self) -> peritus_types::Sha256Digest {
        self.state_digest
    }
}

impl CanonicalEncode for GateStateFrame {
    const FAMILY: u16 = 52;
    const SCHEMA_VERSION: u16 = 1;

    fn encode_payload(&self, writer: &mut CanonicalWriter) -> Result<(), CodecError> {
        super::write_id(writer, self.run_id.as_bytes())?;
        super::write_digest(writer, self.plan_digest)?;
        super::write_revision(writer, self.revision)?;
        super::write_digest(writer, self.snapshot_digest)?;
        writer.write_u16(self.maximum_attempts)?;
        writer.write_u8(crate::canonical::run_phase_tag(self.phase))?;
        writer.write_u64(self.sequence.get())?;
        super::write_id(writer, self.last_event_id.as_bytes())?;
        super::write_digest(writer, self.state_digest)?;
        writer.write_collection_len(self.slots.len())?;
        for slot in &self.slots {
            write_slot(writer, slot)?;
        }
        writer.write_collection_len(self.used_executions.len())?;
        for execution in &self.used_executions {
            super::write_id(writer, execution.as_bytes())?;
        }
        writer.write_collection_len(self.used_actions.len())?;
        for action in &self.used_actions {
            super::write_id(writer, action.as_bytes())?;
        }
        writer.write_option_tag(self.terminal.is_some())?;
        if let Some(terminal) = &self.terminal {
            writer.write_u8(crate::canonical::terminal_kind_tag(terminal.kind))?;
            writer.write_collection_len(terminal.non_passing.len())?;
            for gate in &terminal.non_passing {
                super::write_id(writer, gate.as_bytes())?;
            }
            super::write_digest(writer, terminal.digest)?;
        }
        Ok(())
    }
}

impl CanonicalDecode for GateStateFrame {
    const FAMILY: u16 = 52;
    const SCHEMA_VERSION: u16 = 1;

    fn decode_payload(reader: &mut CanonicalReader<'_>) -> Result<Self, CodecError> {
        let run_id = super::read_run_id(reader)?;
        let plan_digest = super::read_digest(reader)?;
        let revision = super::read_revision(reader)?;
        let snapshot_digest = super::read_digest(reader)?;
        let maximum_offset = reader.offset();
        let maximum_attempts = reader.read_u16()?;
        if maximum_attempts == 0 {
            return Err(invalid(maximum_offset));
        }
        let phase = read_run_phase(reader)?;
        let sequence_offset = reader.offset();
        let sequence =
            EventSequence::new(reader.read_u64()?).map_err(|_| invalid(sequence_offset))?;
        let last_event_id = super::read_event_id(reader)?;
        let state_digest = super::read_digest(reader)?;
        let slot_count = reader.read_collection_len()?;
        if slot_count > crate::descriptor::MAX_GATES_PER_RUN {
            return Err(invalid(reader.offset()));
        }
        let mut slots = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            slots.push(read_slot(reader)?);
        }
        if slots.windows(2).any(|pair| pair[0].gate_id >= pair[1].gate_id) {
            return Err(invalid(reader.offset()));
        }
        let execution_count = reader.read_collection_len()?;
        if execution_count > crate::descriptor::MAX_TOTAL_GATE_ATTEMPTS {
            return Err(invalid(reader.offset()));
        }
        let mut used_executions = Vec::with_capacity(execution_count);
        for _ in 0..execution_count {
            used_executions.push(super::read_execution_id(reader)?);
        }
        if used_executions.iter().copied().collect::<BTreeSet<_>>().len() != used_executions.len() {
            return Err(invalid(reader.offset()));
        }
        let action_count = reader.read_collection_len()?;
        if action_count > crate::descriptor::MAX_TOTAL_GATE_ATTEMPTS
            || action_count != execution_count
        {
            return Err(invalid(reader.offset()));
        }
        let mut used_actions = Vec::with_capacity(action_count);
        for _ in 0..action_count {
            used_actions.push(super::read_action_id(reader)?);
        }
        if used_actions.iter().copied().collect::<BTreeSet<_>>().len() != used_actions.len() {
            return Err(invalid(reader.offset()));
        }
        let terminal = reader.read_option_tag()?.then(|| read_terminal(reader)).transpose()?;
        if (phase == GateRunPhase::Terminal) != terminal.is_some() {
            return Err(invalid(reader.offset()));
        }
        Ok(Self {
            run_id,
            plan_digest,
            revision,
            snapshot_digest,
            maximum_attempts,
            phase,
            sequence,
            last_event_id,
            state_digest,
            slots,
            used_executions,
            used_actions,
            terminal,
        })
    }
}

fn write_slot(writer: &mut CanonicalWriter, slot: &CheckpointSlot) -> Result<(), CodecError> {
    super::write_id(writer, slot.gate_id.as_bytes())?;
    writer.write_u8(crate::canonical::slot_phase_tag(slot.phase))?;
    writer.write_u16(slot.attempts)?;
    writer.write_option_tag(slot.active.is_some())?;
    if let Some(active) = slot.active {
        super::write_attempt(writer, active)?;
    }
    writer.write_option_tag(slot.result.is_some())?;
    if let Some(result) = &slot.result {
        super::write_result(writer, result)?;
    }
    writer.write_option_tag(slot.result_event.is_some())?;
    if let Some(result_event) = slot.result_event {
        super::write_id(writer, result_event.as_bytes())?;
    }
    writer.write_option_tag(slot.evidence.is_some())?;
    if let Some(evidence) = &slot.evidence {
        super::write_receipt(writer, evidence)?;
    }
    writer.write_option_tag(slot.blocked_by.is_some())?;
    if let Some(blocked_by) = slot.blocked_by {
        super::write_id(writer, blocked_by.as_bytes())?;
    }
    Ok(())
}

fn read_slot(reader: &mut CanonicalReader<'_>) -> Result<CheckpointSlot, CodecError> {
    let gate_id = super::read_gate_id(reader)?;
    let phase = read_slot_phase(reader)?;
    let attempts = reader.read_u16()?;
    let active = reader.read_option_tag()?.then(|| super::read_attempt(reader)).transpose()?;
    if active.is_some_and(|attempt| attempt.ordinal().get() != attempts) {
        return Err(invalid(reader.offset()));
    }
    let result = reader.read_option_tag()?.then(|| super::read_result(reader)).transpose()?;
    let result_event =
        reader.read_option_tag()?.then(|| super::read_event_id(reader)).transpose()?;
    if result.is_some() != result_event.is_some() {
        return Err(invalid(reader.offset()));
    }
    let evidence = reader.read_option_tag()?.then(|| super::read_receipt(reader)).transpose()?;
    let blocked_by = reader.read_option_tag()?.then(|| super::read_gate_id(reader)).transpose()?;
    Ok(CheckpointSlot {
        gate_id,
        phase,
        attempts,
        active,
        result,
        result_event,
        evidence,
        blocked_by,
    })
}

fn read_terminal(reader: &mut CanonicalReader<'_>) -> Result<CheckpointTerminal, CodecError> {
    let kind = read_terminal_kind(reader)?;
    let count = reader.read_collection_len()?;
    if count > crate::descriptor::MAX_GATES_PER_RUN {
        return Err(invalid(reader.offset()));
    }
    let mut non_passing = Vec::with_capacity(count);
    for _ in 0..count {
        non_passing.push(super::read_gate_id(reader)?);
    }
    if non_passing.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid(reader.offset()));
    }
    Ok(CheckpointTerminal { kind, non_passing, digest: super::read_digest(reader)? })
}

fn read_run_phase(reader: &mut CanonicalReader<'_>) -> Result<GateRunPhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateRunPhase::Active),
        2 => Ok(GateRunPhase::Cancelling),
        3 => Ok(GateRunPhase::Terminal),
        4 => Ok(GateRunPhase::Paused(crate::GateResumePhase::Active)),
        5 => Ok(GateRunPhase::Paused(crate::GateResumePhase::Cancelling)),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn read_slot_phase(reader: &mut CanonicalReader<'_>) -> Result<GateSlotPhase, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateSlotPhase::Pending),
        2 => Ok(GateSlotPhase::Prepared),
        3 => Ok(GateSlotPhase::Dispatched),
        4 => Ok(GateSlotPhase::RecoveryPending),
        5 => Ok(GateSlotPhase::RetryPending),
        6 => Ok(GateSlotPhase::EvidencePending),
        7 => Ok(GateSlotPhase::Passed),
        8 => Ok(GateSlotPhase::Failed),
        9 => Ok(GateSlotPhase::Blocked),
        10 => Ok(GateSlotPhase::Cancelled),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

fn read_terminal_kind(reader: &mut CanonicalReader<'_>) -> Result<GateTerminalKind, CodecError> {
    let offset = reader.offset();
    match reader.read_u8()? {
        1 => Ok(GateTerminalKind::Passed),
        2 => Ok(GateTerminalKind::Failed),
        3 => Ok(GateTerminalKind::Cancelled),
        4 => Ok(GateTerminalKind::Indeterminate),
        _ => Err(CodecError::at(CodecErrorKind::UnknownTag, offset)),
    }
}

const fn invalid(offset: usize) -> CodecError {
    CodecError::at(CodecErrorKind::InvalidDomainValue, offset)
}
