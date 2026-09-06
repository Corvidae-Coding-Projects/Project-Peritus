//! Canonical source-backed entry fields; model deltas cannot set host-derived status.

use peritus_codec::{CanonicalReader, CanonicalWriter, sha256};
use crate::{ContextLimits, bind_context_content};
use super::{WorkingCodecError, count, fields};
use super::super::{ObservationId, WorkingEntry, WorkingEntryKind, WorkingEntryStatus,
    WorkingLimits, WorkingLinks};

pub(super) fn write_entry(w: &mut CanonicalWriter, entry: &WorkingEntry) -> Result<(), WorkingCodecError> {
    w.write_fixed(entry.id().as_bytes())?;
    w.write_u8(match entry.kind() {
        WorkingEntryKind::Observation => 0, WorkingEntryKind::Assertion => 1,
        WorkingEntryKind::Hypothesis => 2, WorkingEntryKind::Decision => 3,
        WorkingEntryKind::FailedApproach => 4, WorkingEntryKind::Plan => 5,
        WorkingEntryKind::Glossary => 6,
    })?;
    w.write_u8(match entry.status() {
        WorkingEntryStatus::Open => 0, WorkingEntryStatus::Contradicted => 1,
        WorkingEntryStatus::Resolved => 2, WorkingEntryStatus::Stale => 3,
        WorkingEntryStatus::Superseded => 4,
    })?;
    w.write_bytes(entry.content().bytes())?;
    write_sources(w, entry.links().supports())?;
    write_sources(w, entry.links().contradicts())?;
    w.write_collection_len(entry.links().depends_on().len())?;
    for id in entry.links().depends_on() { w.write_fixed(id.as_bytes())?; }
    fields::write_validity(w, entry.validity())?;
    w.write_option_tag(entry.supersedes().is_some())?;
    if let Some(id) = entry.supersedes() { w.write_fixed(id.as_bytes())?; }
    w.write_u64(entry.stale_through)?;
    Ok(())
}

pub(super) fn read_entry(r: &mut CanonicalReader<'_>, limits: WorkingLimits) -> Result<WorkingEntry, WorkingCodecError> {
    let id = fields::read_id(r)?;
    let kind = match r.read_u8()? {
        0 => WorkingEntryKind::Observation, 1 => WorkingEntryKind::Assertion,
        2 => WorkingEntryKind::Hypothesis, 3 => WorkingEntryKind::Decision,
        4 => WorkingEntryKind::FailedApproach, 5 => WorkingEntryKind::Plan,
        6 => WorkingEntryKind::Glossary, _ => return Err(WorkingCodecError::InvalidValue),
    };
    let status = match r.read_u8()? {
        0 => WorkingEntryStatus::Open, 1 => WorkingEntryStatus::Contradicted,
        2 => WorkingEntryStatus::Resolved, 3 => WorkingEntryStatus::Stale,
        4 => WorkingEntryStatus::Superseded, _ => return Err(WorkingCodecError::InvalidValue),
    };
    let bytes = r.read_bytes()?;
    let content_limits = ContextLimits::new(limits.entries(), limits.entry_bytes(), limits.links(), 5)
        .map_err(|_| WorkingCodecError::InvalidValue)?;
    let content = bind_context_content(bytes.to_vec(), sha256(bytes), content_limits)
        .map_err(|_| WorkingCodecError::InvalidValue)?;
    let supports = read_sources(r, limits.links())?;
    let contradicts = read_sources(r, limits.links())?;
    let length = count(r, limits.links())?;
    let mut dependencies = Vec::with_capacity(length);
    for _ in 0..length { dependencies.push(fields::read_id(r)?); }
    let links = WorkingLinks::new(supports, contradicts, dependencies, limits)?;
    let validity = fields::read_validity(r, limits)?;
    let mut entry = WorkingEntry::new(id, kind, content, links, validity, limits)?;
    if r.read_option_tag()? { entry = entry.with_supersedes(fields::read_id(r)?)?; }
    entry.status = status;
    entry.stale_through = r.read_u64()?;
    Ok(entry)
}

fn write_sources(w: &mut CanonicalWriter, sources: &[ObservationId]) -> Result<(), WorkingCodecError> {
    w.write_collection_len(sources.len())?;
    for source in sources { w.write_u64(source.get())?; }
    Ok(())
}
fn read_sources(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<Vec<ObservationId>, WorkingCodecError> {
    let length = count(r, maximum)?;
    let mut sources = Vec::with_capacity(length);
    for _ in 0..length { sources.push(ObservationId::new(r.read_u64()?)?); }
    Ok(sources)
}
