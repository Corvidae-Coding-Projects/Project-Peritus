//! Fixed-memory selection resolution; source identity is hashed independently by the caller.

use super::{Selection, Sha256Digest, WorkspaceError, changed, invalid, snapshot_io};
use sha2::{Digest as _, Sha256};
use std::io::Write;

pub(super) struct Scan {
    selection: Selection,
    maximum: Option<u64>,
    offset: u64,
    source_size: u64,
    // A source can end with a newline at the last representable byte; its next line is EOF.
    line: u128,
    last_seen_line: u128,
    range: Option<(u64, u64)>,
    selected: u64,
    digest: Sha256,
}
impl Scan {
    pub(super) fn new(
        selection: Selection,
        maximum: Option<u64>,
        source_size: u64,
    ) -> Result<Self, WorkspaceError> {
        match selection {
            Selection::All if maximum.is_some_and(|limit| source_size > limit) => {
                return Err(invalid(
                    "whole file exceeds caller capacity; use streaming inspection",
                ));
            }
            Selection::Bytes { start, end }
                if end > source_size || maximum.is_some_and(|limit| end - start > limit) =>
            {
                return Err(invalid("byte range is absent or exceeds caller inclusion capacity"));
            }
            _ => {}
        }
        Ok(Self {
            selection,
            maximum,
            offset: 0,
            source_size,
            line: 1,
            last_seen_line: 0,
            range: None,
            selected: 0,
            digest: Sha256::new(),
        })
    }
    pub(super) const fn offset(&self) -> u64 {
        self.offset
    }
    pub(super) fn accept(
        &mut self,
        bytes: &[u8],
        output: &mut dyn Write,
    ) -> Result<(), WorkspaceError> {
        let end = self.offset.checked_add(bytes.len() as u64).ok_or_else(changed)?;
        if end > self.source_size {
            return Err(changed());
        }
        let interval = match self.selection {
            Selection::All => Some((0, bytes.len())),
            Selection::Bytes { start, end: last } => {
                let first = start.max(self.offset);
                let last = last.min(end);
                if first < last {
                    Some((
                        usize::try_from(first - self.offset).map_err(|_| changed())?,
                        usize::try_from(last - self.offset).map_err(|_| changed())?,
                    ))
                } else {
                    None
                }
            }
            Selection::Lines { first, last } => self.line_interval(bytes, first, last)?,
        };
        if let Some((first, last)) = interval {
            let selected = self.selected.checked_add((last - first) as u64).ok_or_else(changed)?;
            if self.maximum.is_some_and(|limit| selected > limit) {
                return Err(invalid(
                    "selected lines exceed caller capacity; use streaming inspection",
                ));
            }
            output.write_all(&bytes[first..last]).map_err(snapshot_io)?;
            self.digest.update(&bytes[first..last]);
            self.selected = selected;
            let range = (self.offset + first as u64, self.offset + last as u64);
            match &mut self.range {
                Some((_, last)) => *last = range.1,
                None => self.range = Some(range),
            }
        }
        self.offset = end;
        Ok(())
    }
    fn line_interval(
        &mut self,
        bytes: &[u8],
        first: u64,
        last: u64,
    ) -> Result<Option<(usize, usize)>, WorkspaceError> {
        let (first, last) = (u128::from(first), u128::from(last));
        if self.line > last {
            return Ok(None);
        }
        let mut interval = None;
        for (index, byte) in bytes.iter().enumerate() {
            self.last_seen_line = self.line;
            if self.line >= first {
                match &mut interval {
                    Some((_, end)) => *end = index + 1,
                    None => interval = Some((index, index + 1)),
                }
            }
            if *byte == b'\n' {
                self.line = self.line.checked_add(1).ok_or_else(changed)?;
                if self.line > last {
                    break;
                }
            }
        }
        Ok(interval)
    }
    pub(super) fn finish(self) -> Result<((u64, u64), Sha256Digest), WorkspaceError> {
        if let Selection::Lines { last, .. } = self.selection
            && self.last_seen_line < u128::from(last)
        {
            return Err(invalid("selected line range does not exist in the complete source"));
        }
        Ok((self.range.unwrap_or((0, 0)), Sha256Digest::new(self.digest.finalize().into())))
    }
}

#[cfg(test)]
mod tests;
