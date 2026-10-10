//! Bounded-memory UTF-8 line scanner used by complete search traversals.

use sha2::{Digest as _, Sha256};

use super::super::{FsToolError, FsToolOperation, bound_error};
use super::{SearchInput, SearchMatch};
use peritus_patch::WorkspacePath;

pub(super) struct SearchFile<'a> {
    pub(super) input: &'a SearchInput,
    path: &'a WorkspacePath,
    needle: String,
    pub(super) matches: Vec<SearchMatch>,
    pub(super) global_match: u64,
    match_end: u64,
    pub(super) file_match_count: u64,
    pub(super) match_digest: Sha256,
    line_number: u64,
    line: String,
    line_base: u64,
    scan_from: usize,
    utf8_pending: Vec<u8>,
    pub(super) binary: bool,
    pub(super) failure: Option<FsToolError>,
}

impl<'a> SearchFile<'a> {
    pub(super) fn new(
        path: &'a WorkspacePath,
        input: &'a SearchInput,
        global_match: u64,
        match_end: u64,
    ) -> Self {
        Self {
            input,
            path,
            needle: if input.case_sensitive {
                input.literal.clone()
            } else {
                input.literal.to_ascii_lowercase()
            },
            matches: Vec::new(),
            global_match,
            match_end,
            file_match_count: 0,
            match_digest: Sha256::new(),
            line_number: 1,
            line: String::new(),
            line_base: 0,
            scan_from: 0,
            utf8_pending: Vec::new(),
            binary: false,
            failure: None,
        }
    }

    pub(super) fn accept_bytes(&mut self, bytes: &[u8]) {
        if self.binary || self.failure.is_some() {
            return;
        }
        let mut combined = std::mem::take(&mut self.utf8_pending);
        combined.extend_from_slice(bytes);
        match std::str::from_utf8(&combined) {
            Ok(text) => self.accept_text(text),
            Err(error) => {
                if let Ok(text) = std::str::from_utf8(&combined[..error.valid_up_to()]) {
                    self.accept_text(text);
                }
                if error.error_len().is_some() {
                    self.binary = true;
                    self.line.clear();
                    self.utf8_pending.clear();
                } else {
                    self.utf8_pending.extend_from_slice(&combined[error.valid_up_to()..]);
                }
            }
        }
    }

    fn accept_text(&mut self, text: &str) {
        for segment in text.split_inclusive('\n') {
            let terminated = segment.ends_with('\n');
            self.line.push_str(segment.strip_suffix('\n').unwrap_or(segment));
            self.process_line(terminated, terminated);
            if self.failure.is_some() {
                return;
            }
            if terminated {
                self.line.clear();
                self.line_base = 0;
                self.scan_from = 0;
                let Some(line_number) = self.line_number.checked_add(1) else {
                    self.failure =
                        Some(bound_error(FsToolOperation::Search, "line number overflowed"));
                    return;
                };
                self.line_number = line_number;
            }
        }
    }

    fn process_line(&mut self, terminated: bool, complete: bool) {
        if self.line.is_empty() && !complete {
            return;
        }
        let view = if terminated {
            self.line.strip_suffix('\r').unwrap_or(&self.line)
        } else {
            self.line.as_str()
        };
        let mut stable_limit = if complete {
            view.len()
        } else {
            view.len().saturating_sub(self.needle.len().saturating_add(512))
        };
        while !view.is_char_boundary(stable_limit) {
            stable_limit -= 1;
        }
        let haystack =
            if self.input.case_sensitive { view.to_owned() } else { view.to_ascii_lowercase() };
        if self.scan_from <= haystack.len() && haystack.is_char_boundary(self.scan_from) {
            let search_start = self.scan_from;
            for (offset, _) in haystack[search_start..].match_indices(&self.needle) {
                let column = search_start + offset;
                if column >= stable_limit {
                    break;
                }
                let Some(column_bytes) =
                    u64::try_from(column).ok().and_then(|value| self.line_base.checked_add(value))
                else {
                    self.failure = Some(bound_error(
                        FsToolOperation::Search,
                        "match column is not representable",
                    ));
                    return;
                };
                let matched = SearchMatch {
                    path: self.path.clone(),
                    line: self.line_number,
                    column_bytes,
                    preview: stream_preview(view, self.line_base, column, self.needle.len()),
                };
                crate::read_digest::update_match(&mut self.match_digest, &matched);
                let Some(count) = self.file_match_count.checked_add(1) else {
                    self.failure =
                        Some(bound_error(FsToolOperation::Search, "file match count overflowed"));
                    return;
                };
                self.file_match_count = count;
                if self.global_match >= self.input.continuation_offset
                    && self.global_match < self.match_end
                {
                    self.matches.push(matched);
                }
                let Some(global_match) = self.global_match.checked_add(1) else {
                    self.failure =
                        Some(bound_error(FsToolOperation::Search, "match continuation overflowed"));
                    return;
                };
                self.global_match = global_match;
                self.scan_from = column.saturating_add(self.needle.len());
            }
        }
        if !terminated {
            self.scan_from = self.scan_from.max(stable_limit);
            let mut discard = self.scan_from.saturating_sub(256);
            while !self.line.is_char_boundary(discard) {
                discard -= 1;
            }
            if discard > 0 {
                self.line.drain(..discard);
                let Some(discard_bytes) = u64::try_from(discard).ok() else {
                    self.failure =
                        Some(bound_error(FsToolOperation::Search, "line byte offset overflowed"));
                    return;
                };
                let Some(line_base) = self.line_base.checked_add(discard_bytes) else {
                    self.failure =
                        Some(bound_error(FsToolOperation::Search, "line byte offset overflowed"));
                    return;
                };
                self.line_base = line_base;
                self.scan_from = self.scan_from.saturating_sub(discard);
            }
        }
    }

    pub(super) fn finish(&mut self) {
        if !self.utf8_pending.is_empty() {
            self.binary = true;
            return;
        }
        if !self.binary && self.failure.is_none() && !self.line.is_empty() {
            self.process_line(false, true);
        }
    }
}

fn stream_preview(line: &str, base: u64, column: usize, match_bytes: usize) -> String {
    const LIMIT: usize = 512;
    let global_column = base.saturating_add(u64::try_from(column).unwrap_or(u64::MAX));
    let global_length = base.saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX));
    let start = global_column.saturating_sub((LIMIT / 2) as u64).max(base);
    let mut start_local = usize::try_from(start - base).unwrap_or(0);
    while !line.is_char_boundary(start_local) {
        start_local = start_local.saturating_sub(1);
    }
    let wanted_end = start
        .saturating_add(LIMIT as u64)
        .max(global_column.saturating_add(u64::try_from(match_bytes).unwrap_or(u64::MAX)))
        .min(global_length);
    let mut end_local =
        usize::try_from(wanted_end.saturating_sub(base)).unwrap_or(line.len()).min(line.len());
    while !line.is_char_boundary(end_local) {
        end_local -= 1;
    }
    line[start_local..end_local].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_flushes_a_short_unterminated_line() {
        let path = WorkspacePath::new("short.txt").expect("path");
        let input = SearchInput::new(None, "needle".to_owned(), true, 8, 1_000_000, 10)
            .expect("search input");
        let mut search = SearchFile::new(&path, &input, 0, 10);
        search.accept_bytes(b"small needle");
        search.finish();

        assert_eq!(search.file_match_count, 1);
        assert_eq!(search.matches[0].column_bytes(), 6);
    }

    #[test]
    fn finish_finds_a_match_split_across_chunks_near_eof() {
        let path = WorkspacePath::new("large.txt").expect("path");
        let input = SearchInput::new(None, "needle".to_owned(), true, 8, 1_000_000, 10)
            .expect("search input");
        let mut search = SearchFile::new(&path, &input, 0, 10);
        let mut first_chunk = vec![b'x'; 64 * 1024];
        first_chunk[64 * 1024 - 3..].copy_from_slice(b"nee");
        search.accept_bytes(&first_chunk);
        search.accept_bytes(b"dle");
        search.finish();

        assert_eq!(search.file_match_count, 1);
        assert_eq!(search.matches[0].column_bytes(), (64 * 1024 - 3) as u64);
    }

    #[test]
    fn repeated_matches_keep_exact_columns_across_chunk_boundaries() {
        let path = WorkspacePath::new("repeated.txt").expect("path");
        let input = SearchInput::new(None, "needle".to_owned(), true, 8, 1_000_000, 10)
            .expect("search input");
        let source = format!("{}needle needle needle\n", "x".repeat(65_530));
        for width in [1, 7, 65_536, source.len()] {
            let mut search = SearchFile::new(&path, &input, 0, 10);
            for chunk in source.as_bytes().chunks(width) {
                search.accept_bytes(chunk);
            }
            search.finish();
            let columns = search.matches.iter().map(SearchMatch::column_bytes).collect::<Vec<_>>();
            assert_eq!(columns, [65_530, 65_537, 65_544], "chunk width {width}");
            assert_eq!(search.file_match_count, 3);
        }
    }
}
