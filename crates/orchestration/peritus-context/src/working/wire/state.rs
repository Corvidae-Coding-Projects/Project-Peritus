//! Exact working snapshots, with bounded reconstruction and graph/status validation.

use super::super::validation::{invalidate_entries, validate_graph, validate_references};
use super::super::{WorkingBinding, WorkingEntryStatus, WorkingError, WorkingLimits, WorkingState};
use super::{WorkingCodecError, count, entry, fields, reader_version, wide_count, writer};
use peritus_codec::{CanonicalReader, CanonicalWriter};

/// Encodes exact retained state, including counterevidence and invalidation boundaries.
///
/// # Errors
/// Rejects snapshots wider than the canonical allocation envelope.
pub fn encode_working_state(state: &WorkingState) -> Result<Vec<u8>, WorkingCodecError> {
    let mut w = writer(*b"PWMS")?;
    write_limits(&mut w, state.limits())?;
    fields::write_environment(&mut w, state.environment())?;
    w.write_u64(state.revision())?;
    w.write_u64(state.observations.len() as u64)?;
    for source in &state.observations {
        fields::write_source(&mut w, *source)?;
    }
    w.write_u64(state.entries.len() as u64)?;
    for record in &state.entries {
        entry::write_entry(&mut w, record)?;
    }
    super::protocol::write_protocol(&mut w, &state.protocol)?;
    Ok(w.into_bytes())
}

/// Restores a canonical snapshot only within an explicitly expected stable lineage.
///
/// The artifact host separately verifies its digest and publication manifest. This routine
/// rechecks all structural invariants; it does not establish the truth of model-authored text.
///
/// # Errors
/// Rejects unsupported versions, trailing bytes, oversized limits, bad sources/graphs/statuses,
/// or cross-run/workspace/task/role state. The host refreshes conversation/workspace after load.
pub fn decode_working_state(
    bytes: &[u8],
    expected: WorkingBinding,
    maximum: WorkingLimits,
) -> Result<WorkingState, WorkingCodecError> {
    let (mut r, version) = reader_version(bytes, *b"PWMS")?;
    let limits = read_limits(&mut r, maximum, version)?;
    let environment = fields::read_environment(&mut r, limits)?;
    if !expected.same_lineage(environment.binding()) {
        return Err(WorkingError::BindingMismatch.into());
    }
    let mut state = WorkingState::new(environment, limits)?;
    state.revision = r.read_u64()?;
    let sources = if version == 1 {
        count(&mut r, limits.observations())?
    } else {
        wide_count(&mut r, limits.observations())?
    };
    for sequence in 0..sources {
        let source = fields::read_source(&mut r)?;
        if source.id().get() != sequence as u64 + 1 {
            return Err(WorkingError::SourceSequence.into());
        }
        state.observations.push(source)?;
    }
    let entries = if version == 1 {
        count(&mut r, limits.entries())?
    } else {
        wide_count(&mut r, limits.entries())?
    };
    for _ in 0..entries {
        let record = entry::read_entry(&mut r, limits)?;
        if state.entries.last().is_some_and(|old| old.id() >= record.id()) {
            return Err(WorkingError::NonCanonicalOrder.into());
        }
        state.entries.push(record);
    }
    state.protocol = super::protocol::read_protocol(&mut r, limits)?;
    r.finish()?;
    validate_snapshot(&state)?;
    Ok(state)
}

pub(super) fn validate_snapshot(state: &WorkingState) -> Result<(), WorkingCodecError> {
    if state.revision < state.through_observation()
        || (!state.entries.is_empty() && state.revision == state.through_observation())
    {
        return Err(WorkingCodecError::InvalidValue);
    }
    validate_graph(&state.entries)?;
    super::super::protocol::validate_protocol(state, &state.protocol)?;
    let mut successors = std::collections::BTreeMap::new();
    for record in &state.entries {
        if let Some(previous) = record.supersedes() {
            *successors.entry(previous).or_insert(0usize) += 1;
        }
    }
    for record in &state.entries {
        validate_references(state, record)?;
        if record.stale_through > state.through_observation() {
            return Err(WorkingCodecError::InvalidValue);
        }
        let successors = successors.get(&record.id()).copied().unwrap_or(0);
        if successors > 1
            || (successors == 1) != (record.status() == WorkingEntryStatus::Superseded)
        {
            return Err(WorkingCodecError::InvalidValue);
        }
    }
    let invalidated =
        invalidate_entries(&state.entries, state.environment(), state.through_observation());
    if invalidated
        .iter()
        .zip(&state.entries)
        .any(|(checked, original)| checked.status() != original.status())
    {
        return Err(WorkingError::StaleEntry.into());
    }
    Ok(())
}

pub(super) fn write_limits(
    w: &mut CanonicalWriter,
    limits: WorkingLimits,
) -> Result<(), WorkingCodecError> {
    for value in [
        limits.observations(),
        limits.entries(),
        limits.entry_bytes(),
        limits.links(),
        limits.operations(),
    ] {
        w.write_u64(value as u64)?;
    }
    Ok(())
}
pub(super) fn read_limits(
    r: &mut CanonicalReader<'_>,
    maximum: WorkingLimits,
    version: u16,
) -> Result<WorkingLimits, WorkingCodecError> {
    let mut values = [0; 5];
    for value in &mut values {
        *value = if version == 1 {
            r.read_u32()? as usize
        } else {
            usize::try_from(r.read_u64()?).map_err(|_| WorkingCodecError::InvalidValue)?
        };
    }
    if values
        .iter()
        .zip([
            maximum.observations(),
            maximum.entries(),
            maximum.entry_bytes(),
            maximum.links(),
            maximum.operations(),
        ])
        .any(|(value, max)| *value > max)
    {
        return Err(WorkingError::Capacity.into());
    }
    Ok(WorkingLimits::new(values[0], values[1], values[2], values[3], values[4])?)
}
