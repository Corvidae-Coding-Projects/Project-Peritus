//! Incremental UTF-8 validation and per-line literal matching for workspace search.

use std::{
    fs,
    io::{BufRead as _, BufReader},
    path::Path,
};

use peritus_agent::DeveloperLoopError;

use crate::developer_tools::{inspection_cancellation::InspectionCancellation, path::tool};

pub(super) fn scan_file<F>(
    path: &Path,
    query: &[u8],
    prefix: &[usize],
    cancellation: &InspectionCancellation,
    visit: F,
) -> Result<bool, DeveloperLoopError>
where
    F: FnMut(usize, u64, u64) -> Result<bool, DeveloperLoopError>,
{
    let file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    MatchScanner { query, prefix, cancellation, visit }.scan(BufReader::new(file))
}

pub(super) fn validate_text_file(
    path: &Path,
    cancellation: &InspectionCancellation,
) -> Result<(), DeveloperLoopError> {
    let file = fs::File::open(path).map_err(|error| tool(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut pending = Vec::with_capacity(4);
    loop {
        cancellation.check()?;
        let available = reader.fill_buf().map_err(|error| tool(error.to_string()))?;
        if available.is_empty() {
            break;
        }
        if available.contains(&0) {
            return Err(tool("binary file contains a NUL byte and was skipped"));
        }
        let consumed = available.len();
        pending.extend_from_slice(available);
        match std::str::from_utf8(&pending) {
            Ok(_) => pending.clear(),
            Err(error) if error.error_len().is_none() => {
                let valid = error.valid_up_to();
                pending.drain(..valid);
                if pending.len() > 3 {
                    return Err(tool("file is not valid UTF-8 text and was skipped"));
                }
            }
            Err(_) => return Err(tool("file is not valid UTF-8 text and was skipped")),
        }
        reader.consume(consumed);
    }
    if !pending.is_empty() {
        return Err(tool("file ends inside a UTF-8 character and was skipped"));
    }
    Ok(())
}

struct MatchScanner<'a, F> {
    query: &'a [u8],
    prefix: &'a [usize],
    cancellation: &'a InspectionCancellation,
    visit: F,
}

impl<F> MatchScanner<'_, F>
where
    F: FnMut(usize, u64, u64) -> Result<bool, DeveloperLoopError>,
{
    fn scan(mut self, mut reader: BufReader<fs::File>) -> Result<bool, DeveloperLoopError> {
        let mut line = 1_usize;
        let mut offset = 0_u64;
        let mut matched = 0_usize;
        let mut hit = None;
        loop {
            self.cancellation.check()?;
            let available = reader.fill_buf().map_err(|error| tool(error.to_string()))?;
            if available.is_empty() {
                if let Some(start) = hit {
                    return (self.visit)(line, start, offset);
                }
                return Ok(true);
            }
            let count = available.len();
            for byte in available {
                if *byte == b'\n' {
                    if let Some(start) = hit
                        && !(self.visit)(line, start, offset)?
                    {
                        return Ok(false);
                    }
                    line = line.checked_add(1).ok_or_else(|| tool("line number overflow"))?;
                    offset = 0;
                    matched = 0;
                    hit = None;
                    continue;
                }
                offset = offset.checked_add(1).ok_or_else(|| tool("line byte count overflow"))?;
                if hit.is_some() {
                    continue;
                }
                while matched > 0 && self.query[matched] != *byte {
                    matched = self.prefix[matched - 1];
                }
                if self.query[matched] == *byte {
                    matched += 1;
                }
                if matched == self.query.len() {
                    hit = Some(offset.saturating_sub(self.query.len() as u64));
                }
            }
            reader.consume(count);
        }
    }
}

pub(super) fn kmp_prefix(query: &[u8]) -> Vec<usize> {
    let mut prefix = vec![0; query.len()];
    let mut matched = 0;
    for index in 1..query.len() {
        while matched > 0 && query[index] != query[matched] {
            matched = prefix[matched - 1];
        }
        if query[index] == query[matched] {
            matched += 1;
            prefix[index] = matched;
        }
    }
    prefix
}

pub(super) fn utf8_prefix_len(value: &str, maximum: usize) -> usize {
    value
        .char_indices()
        .take_while(|(offset, character)| offset.saturating_add(character.len_utf8()) <= maximum)
        .map(|(offset, character)| offset + character.len_utf8())
        .last()
        .unwrap_or(0)
}
