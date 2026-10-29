//! Rebuildable source projections and an independent scan path for qualification/rollback.

use super::{ArchivedObservation, DeveloperLoopError, LocalMemory};
use crate::local_context::{error, record::ArchiveKind};
use serde::Serialize;

/// Numeric work at the source-projection boundary, never persisted as authoritative state.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub(in crate::local_context) struct SourceProjectionWork {
    pub(super) archive_records: u64,
    pub(super) suffix_records: u64,
    pub(super) tool_identity_records: u64,
    pub(super) latest_tool_records: u64,
    pub(super) accounting_records: u64,
    pub(super) rank_keys: u64,
}

impl SourceProjectionWork {
    pub(in crate::local_context) const fn since(self, prior: Self) -> Self {
        Self {
            archive_records: self.archive_records.saturating_sub(prior.archive_records),
            suffix_records: self.suffix_records.saturating_sub(prior.suffix_records),
            tool_identity_records: self
                .tool_identity_records
                .saturating_sub(prior.tool_identity_records),
            latest_tool_records: self.latest_tool_records.saturating_sub(prior.latest_tool_records),
            accounting_records: self.accounting_records.saturating_sub(prior.accounting_records),
            rank_keys: self.rank_keys.saturating_sub(prior.rank_keys),
        }
    }
}

impl LocalMemory {
    fn source_visit(&self, field: fn(&mut SourceProjectionWork) -> &mut u64) {
        if self.measure_work {
            let mut work = self.source_work.get();
            let count = field(&mut work);
            *count = count.saturating_add(1);
            self.source_work.set(work);
        }
    }

    pub(in crate::local_context) fn rank_key(&self) {
        self.source_visit(|work| &mut work.rank_keys);
    }

    pub(in crate::local_context) fn archived(
        &self,
        sequence: u64,
    ) -> Result<&ArchivedObservation, DeveloperLoopError> {
        if self.measure_work {
            self.archive_lookups.set(self.archive_lookups.get().saturating_add(1));
        }
        let index = usize::try_from(
            sequence.checked_sub(1).ok_or_else(|| error("invalid observation handle"))?,
        )
        .map_err(|_| error("observation handle overflow"))?;
        let source = self
            .sources
            .get(index)
            .inspect(|_| self.source_visit(|work| &mut work.archive_records));
        source
            .filter(|source| source.sequence == sequence)
            .ok_or_else(|| error("observation is unavailable"))
    }

    pub(in crate::local_context) fn sources_after(
        &self,
        sequence: u64,
    ) -> impl Iterator<Item = &ArchivedObservation> {
        self.source_range_after(sequence, self.indexed_sources)
    }

    // This policy path already used checked suffix access before I1; do not degrade it in
    // the reference mode or count an unrelated policy change as an indexing improvement.
    pub(in crate::local_context) fn source_prefix_after(
        &self,
        sequence: u64,
    ) -> impl Iterator<Item = &ArchivedObservation> {
        self.source_range_after(sequence, true)
    }

    fn source_range_after(
        &self,
        sequence: u64,
        indexed: bool,
    ) -> impl Iterator<Item = &ArchivedObservation> {
        let start = if indexed { usize::try_from(sequence).unwrap_or(usize::MAX) } else { 0 };
        self.sources.iter().skip(start).filter(move |source| {
            self.source_visit(|work| &mut work.suffix_records);
            source.sequence > sequence
        })
    }

    pub(in crate::local_context) fn ranking_source(
        &self,
        sequence: u64,
    ) -> Option<&ArchivedObservation> {
        if self.indexed_sources {
            return self.archived(sequence).ok();
        }
        self.sources.iter().find(|source| {
            self.source_visit(|work| &mut work.archive_records);
            source.sequence == sequence
        })
    }

    pub(in crate::local_context) fn next_tool_sequence(&self) -> Result<u64, DeveloperLoopError> {
        let last = if self.indexed_sources {
            self.latest_tool_sequence.get(&self.transcript.invocation).copied()
        } else {
            self.sources
                .iter()
                .rev()
                .find(|source| {
                    self.source_visit(|work| &mut work.latest_tool_records);
                    source.invocation == self.transcript.invocation
                        && source.kind == ArchiveKind::ToolOutput
                })
                .and_then(|source| source.tool_sequence)
        };
        last.unwrap_or(0).checked_add(1).ok_or_else(|| error("tool sequence overflow"))
    }

    pub(in crate::local_context) fn prior_tool_source(
        &self,
        invocation: u64,
        sequence: u64,
    ) -> Result<Option<&ArchivedObservation>, DeveloperLoopError> {
        if !self.indexed_sources {
            return Ok(self.sources.iter().find(|source| {
                self.source_visit(|work| &mut work.tool_identity_records);
                source.invocation == invocation && source.tool_sequence == Some(sequence)
            }));
        }
        let Some(index) = self.tool_sources.get(&(invocation, sequence)) else { return Ok(None) };
        let source =
            self.sources.get(*index).ok_or_else(|| error("tool source index unavailable"))?;
        self.source_visit(|work| &mut work.tool_identity_records);
        if source.invocation != invocation || source.tool_sequence != Some(sequence) {
            return Err(error("tool source index mismatch"));
        }
        Ok(Some(source))
    }

    pub(in crate::local_context) fn archived_bytes(&self) -> Result<u64, DeveloperLoopError> {
        if self.indexed_sources {
            return self.archive_bytes.ok_or_else(|| error("archive accounting overflow"));
        }
        self.sources.iter().try_fold(0_u64, |total, source| {
            self.source_visit(|work| &mut work.accounting_records);
            total
                .checked_add(source.artifact.bytes)
                .ok_or_else(|| error("archive accounting overflow"))
        })
    }
}
