//! Standalone canonical working-event frames with exact scope checks.

use super::{WorkingCodecError, count, entry, fields, reader, writer};
use super::super::{WorkingBinding, WorkingDelta, WorkingError, WorkingEvent, WorkingLimits};

/// Encodes one source/environment/delta event as a version-one canonical payload.
///
/// # Errors
/// Rejects data that exceeds the canonical allocation envelope.
pub fn encode_working_event(event: &WorkingEvent) -> Result<Vec<u8>, WorkingCodecError> {
    let mut w = writer(*b"PWME")?;
    match event {
        WorkingEvent::Observation { binding, source } => {
            w.write_u8(0)?;
            fields::write_binding(&mut w, *binding)?;
            fields::write_source(&mut w, *source)?;
        }
        WorkingEvent::Refresh { base_revision, environment } => {
            w.write_u8(1)?;
            w.write_u64(*base_revision)?;
            fields::write_environment(&mut w, environment)?;
        }
        WorkingEvent::Delta(delta) => {
            w.write_u8(2)?;
            fields::write_binding(&mut w, delta.binding())?;
            w.write_u64(delta.base_revision())?;
            w.write_collection_len(delta.entries().len())?;
            for proposal in delta.entries() { entry::write_entry(&mut w, proposal)?; }
        }
        WorkingEvent::Protocol(update) => {
            w.write_u8(3)?;
            fields::write_binding(&mut w, update.binding())?;
            w.write_u64(update.base_revision())?;
            super::protocol::write_protocol(&mut w, update.protocol())?;
        }
    }
    Ok(w.into_bytes())
}

/// Decodes a bounded event for one stable lineage. Conversation advancement is reducer-checked.
///
/// # Errors
/// Rejects unknown versions/tags, noncanonical/trailing data, invalid domain values and scope.
pub fn decode_working_event(bytes: &[u8], expected: WorkingBinding, limits: WorkingLimits) -> Result<WorkingEvent, WorkingCodecError> {
    let mut r = reader(bytes, *b"PWME")?;
    let (binding, event) = match r.read_u8()? {
        0 => {
            let binding = fields::read_binding(&mut r)?;
            (binding, WorkingEvent::Observation { binding, source: fields::read_source(&mut r)? })
        }
        1 => {
            let base_revision = r.read_u64()?;
            let environment = fields::read_environment(&mut r, limits)?;
            (environment.binding(), WorkingEvent::Refresh { base_revision, environment })
        }
        2 => {
            let binding = fields::read_binding(&mut r)?;
            let revision = r.read_u64()?;
            let length = count(&mut r, limits.operations())?;
            let mut entries = Vec::with_capacity(length);
            for _ in 0..length { entries.push(entry::read_entry(&mut r, limits)?); }
            (binding, WorkingEvent::Delta(WorkingDelta::new(binding, revision, entries, limits)?))
        }
        3 => {
            let binding = fields::read_binding(&mut r)?;
            let revision = r.read_u64()?;
            let protocol = super::protocol::read_protocol(&mut r, limits)?;
            (binding, WorkingEvent::Protocol(super::super::WorkingProtocolUpdate::new(binding, revision, protocol)))
        }
        _ => return Err(WorkingCodecError::InvalidValue),
    };
    r.finish()?;
    if !expected.same_lineage(binding) { return Err(WorkingError::BindingMismatch.into()); }
    Ok(event)
}
