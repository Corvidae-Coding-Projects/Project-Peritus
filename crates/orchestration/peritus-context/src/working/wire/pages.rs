//! Independently addressable snapshot pages. Page size affects I/O, never admissible history.

use super::super::{WorkingBinding, WorkingError, WorkingLimits, WorkingState};
use super::{WorkingCodecError, count, entry, fields, protocol, reader_version, state, writer};

const RECORDS: usize = 512;
const ENTRIES: usize = 64;

/// Streaming encoder for a complete, ordered working snapshot.
/// Only the current page is allocated. Immutable source pages have stable canonical bytes.
pub struct WorkingStatePages<'a> {
    state: &'a WorkingState,
    stage: u8,
    offset: usize,
}

impl<'a> WorkingStatePages<'a> {
    /// Starts an exact paged snapshot without cloning its history.
    #[must_use]
    pub const fn new(state: &'a WorkingState) -> Self {
        Self { state, stage: 0, offset: 0 }
    }

    fn page(&mut self) -> Result<Vec<u8>, WorkingCodecError> {
        let mut w = writer(*b"PWMP")?;
        w.write_u8(self.stage)?;
        match self.stage {
            0 => {
                state::write_limits(&mut w, self.state.limits())?;
                fields::write_environment(&mut w, self.state.environment())?;
                w.write_u64(self.state.revision())?;
                w.write_u64(self.state.observations.len() as u64)?;
                w.write_u64(self.state.entries.len() as u64)?;
                self.stage = 1;
            }
            1 => {
                let end = self.offset.saturating_add(RECORDS).min(self.state.observations.len());
                w.write_u64(self.offset as u64)?;
                w.write_collection_len(end - self.offset)?;
                for index in self.offset..end {
                    fields::write_source(
                        &mut w,
                        *self.state.observations.get(index).ok_or(WorkingError::MissingSource)?,
                    )?;
                }
                self.offset = end;
                if end == self.state.observations.len() {
                    self.stage = 2;
                    self.offset = 0;
                }
            }
            2 => {
                let end = self.offset.saturating_add(ENTRIES).min(self.state.entries.len());
                w.write_u64(self.offset as u64)?;
                w.write_collection_len(end - self.offset)?;
                for record in &self.state.entries[self.offset..end] {
                    entry::write_entry(&mut w, record)?;
                }
                self.offset = end;
                if end == self.state.entries.len() {
                    self.stage = 3;
                }
            }
            3 => {
                protocol::write_protocol(&mut w, &self.state.protocol)?;
                self.stage = 4;
            }
            _ => return Err(WorkingCodecError::InvalidValue),
        }
        Ok(w.into_bytes())
    }
}

impl Iterator for WorkingStatePages<'_> {
    type Item = Result<Vec<u8>, WorkingCodecError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.stage == 4 {
            return None;
        }
        let result = self.page();
        if result.is_err() {
            self.stage = 4;
        }
        Some(result)
    }
}

/// Ordered, checked reconstruction from independently authenticated snapshot pages.
pub struct WorkingStatePageDecoder {
    binding: WorkingBinding,
    maximum: WorkingLimits,
    state: Option<WorkingState>,
    sources: usize,
    entries: usize,
    stage: u8,
    failed: bool,
}

impl WorkingStatePageDecoder {
    /// Binds reconstruction to the expected run/workspace/task/role.
    #[must_use]
    pub const fn new(binding: WorkingBinding, maximum: WorkingLimits) -> Self {
        Self { binding, maximum, state: None, sources: 0, entries: 0, stage: 0, failed: false }
    }

    /// Consumes the next complete page, rejecting gaps, duplicate pages, or foreign state.
    /// # Errors
    /// Rejects malformed pages and invalid state in the same way as monolithic decoding.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), WorkingCodecError> {
        if self.failed {
            return Err(WorkingCodecError::InvalidValue);
        }
        let result = self.push_page(bytes);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn push_page(&mut self, bytes: &[u8]) -> Result<(), WorkingCodecError> {
        let (mut r, version) = reader_version(bytes, *b"PWMP")?;
        if version != super::VERSION || r.read_u8()? != self.stage {
            return Err(WorkingCodecError::InvalidValue);
        }
        if self.stage == 0 {
            let limits = state::read_limits(&mut r, self.maximum, version)?;
            let environment = fields::read_environment(&mut r, limits)?;
            if !self.binding.same_lineage(environment.binding()) {
                return Err(WorkingError::BindingMismatch.into());
            }
            let mut state = WorkingState::new(environment, limits)?;
            state.revision = r.read_u64()?;
            self.sources = usize::try_from(r.read_u64()?).map_err(|_| WorkingError::Capacity)?;
            self.entries = usize::try_from(r.read_u64()?).map_err(|_| WorkingError::Capacity)?;
            if self.sources > limits.observations() || self.entries > limits.entries() {
                return Err(WorkingError::Capacity.into());
            }
            self.state = Some(state);
            self.stage = 1;
        } else {
            let state = self.state.as_mut().ok_or(WorkingCodecError::InvalidValue)?;
            match self.stage {
                1 => {
                    if r.read_u64()? != state.observations.len() as u64 {
                        return Err(WorkingError::SourceSequence.into());
                    }
                    let size = count(&mut r, RECORDS)?;
                    if size != self.sources.saturating_sub(state.observations.len()).min(RECORDS) {
                        return Err(WorkingCodecError::InvalidValue);
                    }
                    for _ in 0..size {
                        let source = fields::read_source(&mut r)?;
                        if source.id().get() != state.observations.len() as u64 + 1 {
                            return Err(WorkingError::SourceSequence.into());
                        }
                        state.observations.push(source)?;
                    }
                    if state.observations.len() == self.sources {
                        self.stage = 2;
                    }
                }
                2 => {
                    if r.read_u64()? != state.entries.len() as u64 {
                        return Err(WorkingCodecError::InvalidValue);
                    }
                    let size = count(&mut r, ENTRIES)?;
                    if size != self.entries.saturating_sub(state.entries.len()).min(ENTRIES) {
                        return Err(WorkingCodecError::InvalidValue);
                    }
                    for _ in 0..size {
                        let record = entry::read_entry(&mut r, state.limits())?;
                        if state.entries.last().is_some_and(|old| old.id() >= record.id()) {
                            return Err(WorkingError::NonCanonicalOrder.into());
                        }
                        state.entries.push(record);
                    }
                    if state.entries.len() == self.entries {
                        self.stage = 3;
                    }
                }
                3 => {
                    state.protocol = protocol::read_protocol(&mut r, state.limits())?;
                    self.stage = 4;
                }
                _ => return Err(WorkingCodecError::InvalidValue),
            }
        }
        r.finish()?;
        Ok(())
    }

    /// Completes reconstruction and validates all source, graph, and status invariants.
    /// # Errors
    /// Rejects incomplete snapshots or invalid cross-page references.
    pub fn finish(self) -> Result<WorkingState, WorkingCodecError> {
        if self.failed || self.stage != 4 {
            return Err(WorkingCodecError::InvalidValue);
        }
        let state = self.state.ok_or(WorkingCodecError::InvalidValue)?;
        state::validate_snapshot(&state)?;
        Ok(state)
    }
}
