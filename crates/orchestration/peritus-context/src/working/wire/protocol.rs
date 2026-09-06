//! Canonical literal pins and pending-operation dispositions.

use peritus_codec::{CanonicalReader, CanonicalWriter};
use super::{WorkingCodecError, count, fields};
use super::super::{ObservationId, PendingOperationState, WorkingLimits, WorkingPendingOperation, WorkingProtocol};

pub(super) fn write_protocol(w: &mut CanonicalWriter, protocol: &WorkingProtocol) -> Result<(), WorkingCodecError> {
    w.write_collection_len(protocol.requirements().len())?;
    for id in protocol.requirements() { w.write_u64(id.get())?; }
    w.write_collection_len(protocol.pending().len())?;
    for pending in protocol.pending() {
        w.write_fixed(pending.id().as_bytes())?;
        w.write_u64(pending.source().get())?;
        w.write_u8(match pending.state() { PendingOperationState::Proposed => 0, PendingOperationState::Running => 1, PendingOperationState::Unknown => 2 })?;
    }
    Ok(())
}
pub(super) fn read_protocol(r: &mut CanonicalReader<'_>, limits: WorkingLimits) -> Result<WorkingProtocol, WorkingCodecError> {
    let length = count(r, limits.entries())?;
    let mut requirements = Vec::with_capacity(length);
    for _ in 0..length { requirements.push(ObservationId::new(r.read_u64()?)?); }
    let length = count(r, limits.entries())?;
    let mut pending = Vec::with_capacity(length);
    for _ in 0..length {
        let id = fields::read_id(r)?;
        let source = ObservationId::new(r.read_u64()?)?;
        let state = match r.read_u8()? { 0 => PendingOperationState::Proposed, 1 => PendingOperationState::Running, 2 => PendingOperationState::Unknown, _ => return Err(WorkingCodecError::InvalidValue) };
        pending.push(WorkingPendingOperation::new(id, source, state));
    }
    Ok(WorkingProtocol::new(requirements, pending, limits)?)
}
