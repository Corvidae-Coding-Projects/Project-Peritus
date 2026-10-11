//! Streaming structured diff pagination with bounded retained content.

use super::{
    MAX_WORKBENCH_DIFF_PAGE_LINES, WorkbenchDiffHunk, WorkbenchDiffLine, WorkbenchDiffLineKind,
    WorkbenchReviewAnchor, WorkbenchReviewDiffLine, WorkbenchReviewDiffPage,
    WorkbenchReviewDiffQuery, WorkbenchReviewRange, WorkbenchReviewTarget, malformed,
    validate_path,
};
use crate::AppProtocolError;
use peritus_codec::sha256;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};

#[derive(Clone)]
struct Hashes {
    before: Sha256,
    after: Sha256,
    context: Sha256,
}
impl Hashes {
    fn new() -> Self {
        Self { before: Sha256::new(), after: Sha256::new(), context: Sha256::new() }
    }
    fn finish(self) -> (Sha256Digest, Sha256Digest, Sha256Digest) {
        (
            Sha256Digest::new(self.before.finalize().into()),
            Sha256Digest::new(self.after.finalize().into()),
            Sha256Digest::new(self.context.finalize().into()),
        )
    }
}

struct HunkState {
    header: String,
    range: WorkbenchReviewRange,
    hashes: Hashes,
    line_count: u32,
    page_lines: Vec<WorkbenchDiffLine>,
    page_bytes: usize,
    page_exhausted: bool,
}
struct SelectedHunk {
    anchor: WorkbenchReviewAnchor,
    header: String,
    total_lines: u32,
    lines: Vec<WorkbenchDiffLine>,
}
struct FileState {
    path: String,
    hashes: Hashes,
    hunk: Option<HunkState>,
    hunks: u32,
    selected_hunk: Option<SelectedHunk>,
}

/// Parses full totals and anchors while retaining only the requested hunk line page.
#[allow(
    clippy::too_many_lines,
    reason = "the single streaming pass keeps digest, count, and selected-page state coherent"
)]
pub(super) fn parse_page(
    query: WorkbenchReviewDiffQuery,
    candidate: Sha256Digest,
    raw: &str,
    anchors: &[WorkbenchReviewAnchor],
) -> Result<(WorkbenchReviewDiffPage, Vec<bool>), AppProtocolError> {
    if query.revision() == 0 {
        return Err(malformed());
    }
    let diff = sha256(raw.as_bytes());
    let target_file = query.file_offset();
    let target_hunk = query.hunk_offset();
    let target_line = query.line_offset();
    let mut matches = vec![false; anchors.len()];
    let mut file_index = 0_u32;
    let mut total_hunks = 0_u64;
    let mut total_lines = 0_u64;
    let mut selected_file: Option<(WorkbenchReviewAnchor, Option<SelectedHunk>, u32)> = None;
    let mut file = None;
    let mut offset = 0_u32;
    for raw_line in raw.split_inclusive('\n') {
        let raw_len = u32::try_from(raw_line.len()).map_err(|_| malformed())?;
        let line = raw_line
            .strip_suffix('\n')
            .unwrap_or(raw_line)
            .strip_suffix('\r')
            .unwrap_or_else(|| raw_line.strip_suffix('\n').unwrap_or(raw_line));
        if let Some(header) = line.strip_prefix("diff --git ") {
            if let Some(state) = file.take() {
                finish_file(
                    state,
                    query,
                    candidate,
                    diff,
                    file_index,
                    target_file,
                    target_hunk,
                    target_line,
                    &mut total_hunks,
                    &mut total_lines,
                    &mut selected_file,
                    &mut matches,
                    anchors,
                )?;
                file_index = file_index.checked_add(1).ok_or_else(malformed)?;
            }
            file = Some(FileState {
                path: super::parser::path_from_header(header)?,
                hashes: Hashes::new(),
                hunk: None,
                hunks: 0,
                selected_hunk: None,
            });
            offset = offset.checked_add(raw_len).ok_or_else(malformed)?;
            continue;
        }
        let Some(state) = file.as_mut() else {
            offset = offset.checked_add(raw_len).ok_or_else(malformed)?;
            continue;
        };
        if state.hunk.is_none() {
            if let Some(path) = line.strip_prefix("+++ ").and_then(super::parser::diff_path) {
                state.path = path;
            } else if state.path.is_empty()
                && let Some(path) = line.strip_prefix("--- ").and_then(super::parser::diff_path)
            {
                state.path = path;
            }
        }
        if line.starts_with("@@ ") {
            if line.len() > crate::MAX_PRODUCT_DETAIL_BYTES {
                return Err(malformed());
            }
            if let Some(hunk) = state.hunk.take() {
                finalize_hunk(
                    hunk,
                    state,
                    query,
                    candidate,
                    diff,
                    file_index,
                    target_file,
                    target_hunk,
                    target_line,
                    &mut total_hunks,
                    &mut total_lines,
                    &mut matches,
                    anchors,
                )?;
            }
            let mut hunk = HunkState {
                header: line.to_owned(),
                range: super::parser::parse_hunk_range(line)?,
                hashes: Hashes::new(),
                line_count: 0,
                page_lines: Vec::new(),
                page_bytes: 0,
                page_exhausted: false,
            };
            hunk.hashes.context.update(line.as_bytes());
            state.hashes.context.update(line.as_bytes());
            state.hunk = Some(hunk);
        } else if let Some(hunk) = state.hunk.as_mut() {
            let (kind, text) = classify(line);
            hash_line(&mut state.hashes, kind, text);
            hash_line(&mut hunk.hashes, kind, text);
            let index = hunk.line_count;
            hunk.line_count = hunk.line_count.checked_add(1).ok_or_else(malformed)?;
            if file_index == target_file
                && state.hunks == target_hunk
                && index >= target_line
                && hunk.page_lines.len() < MAX_WORKBENCH_DIFF_PAGE_LINES
                && !hunk.page_exhausted
            {
                let prefix = usize::from(matches!(
                    kind,
                    WorkbenchDiffLineKind::Context
                        | WorkbenchDiffLineKind::Added
                        | WorkbenchDiffLineKind::Removed
                ));
                let raw_offset =
                    usize::try_from(offset).map_err(|_| malformed())?.saturating_add(prefix);
                let page_line = WorkbenchDiffLine::from_source(kind, text, raw_offset)?;
                let next_bytes = hunk.page_bytes.saturating_add(page_line.text().len());
                if next_bytes <= 32 * 1024 {
                    hunk.page_bytes = next_bytes;
                    hunk.page_lines.push(page_line);
                } else {
                    // Cursor continuation is positional, so a later shorter line
                    // cannot be admitted after this first budget refusal.
                    hunk.page_exhausted = true;
                }
            }
        } else {
            state.hashes.context.update(line.as_bytes());
            state.hashes.context.update(b"\n");
        }
        offset = offset.checked_add(raw_len).ok_or_else(malformed)?;
    }
    if let Some(state) = file.take() {
        finish_file(
            state,
            query,
            candidate,
            diff,
            file_index,
            target_file,
            target_hunk,
            target_line,
            &mut total_hunks,
            &mut total_lines,
            &mut selected_file,
            &mut matches,
            anchors,
        )?;
        file_index = file_index.checked_add(1).ok_or_else(malformed)?;
    }
    let (file_anchor, selected_hunk, file_hunks) = selected_file.ok_or_else(malformed)?;
    let selected_hunk = if file_hunks == 0 {
        if target_hunk != 0 || target_line != 0 {
            return Err(malformed());
        }
        None
    } else {
        Some(selected_hunk.ok_or_else(malformed)?)
    };
    let source_lines = selected_hunk.as_ref().map_or_else(Vec::new, |value| value.lines.clone());
    let lines = source_lines
        .iter()
        .map(|line| {
            WorkbenchReviewDiffLine::from_wire(
                line.kind(),
                line.text().to_owned(),
                line.raw_offset(),
                line.raw_length(),
                line.is_truncated(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let has_more_file = target_file.checked_add(1).is_some_and(|next| next < file_index);
    let next = if selected_hunk.as_ref().is_some_and(|h| {
        target_line.saturating_add(u32::try_from(h.lines.len()).unwrap_or(u32::MAX)) < h.total_lines
    }) {
        Some(WorkbenchReviewDiffQuery::new(
            query.query(),
            query.run(),
            query.revision(),
            target_file,
            target_hunk,
            target_line.saturating_add(u32::try_from(lines.len()).map_err(|_| malformed())?),
        ))
    } else if file_hunks > target_hunk.saturating_add(1) {
        Some(WorkbenchReviewDiffQuery::new(
            query.query(),
            query.run(),
            query.revision(),
            target_file,
            target_hunk + 1,
            0,
        ))
    } else if has_more_file {
        Some(WorkbenchReviewDiffQuery::new(
            query.query(),
            query.run(),
            query.revision(),
            target_file + 1,
            0,
            0,
        ))
    } else {
        None
    };
    let hunk = selected_hunk
        .map(|value| WorkbenchDiffHunk::new(value.anchor, value.header, value.lines))
        .transpose()?;
    let page = WorkbenchReviewDiffPage::from_wire_parts(
        query,
        candidate,
        diff,
        file_anchor,
        hunk,
        lines,
        file_index,
        total_hunks,
        total_lines,
        next,
    )?;
    Ok((page, matches))
}

fn finalize_hunk(
    hunk: HunkState,
    file: &mut FileState,
    query: WorkbenchReviewDiffQuery,
    candidate: Sha256Digest,
    diff: Sha256Digest,
    file_index: u32,
    target_file: u32,
    target_hunk: u32,
    target_line: u32,
    total_hunks: &mut u64,
    total_lines: &mut u64,
    matches: &mut [bool],
    anchors: &[WorkbenchReviewAnchor],
) -> Result<(), AppProtocolError> {
    if hunk.line_count == 0 {
        return Err(malformed());
    }
    let (before, after, context) = hunk.hashes.finish();
    let anchor = WorkbenchReviewAnchor::new(
        query.run(),
        query.query().workspace(),
        candidate,
        diff,
        file.path.clone(),
        before,
        after,
        context,
        WorkbenchReviewTarget::Hunk,
        hunk.range,
    )?;
    for (index, requested) in anchors.iter().enumerate() {
        if !matches[index] && *requested == anchor {
            matches[index] = true;
        }
    }
    if file_index == target_file && file.hunks == target_hunk {
        if target_line >= hunk.line_count {
            return Err(malformed());
        }
        file.selected_hunk = Some(SelectedHunk {
            anchor,
            header: hunk.header,
            total_lines: hunk.line_count,
            lines: hunk.page_lines,
        });
    }
    *total_hunks = total_hunks.checked_add(1).ok_or_else(malformed)?;
    *total_lines = total_lines.checked_add(u64::from(hunk.line_count)).ok_or_else(malformed)?;
    file.hunks = file.hunks.checked_add(1).ok_or_else(malformed)?;
    Ok(())
}

fn finish_file(
    state: FileState,
    query: WorkbenchReviewDiffQuery,
    candidate: Sha256Digest,
    diff: Sha256Digest,
    index: u32,
    target: u32,
    target_hunk: u32,
    target_line: u32,
    total_hunks: &mut u64,
    total_lines: &mut u64,
    selected: &mut Option<(WorkbenchReviewAnchor, Option<SelectedHunk>, u32)>,
    matches: &mut [bool],
    anchors: &[WorkbenchReviewAnchor],
) -> Result<(), AppProtocolError> {
    let mut state = state;
    if let Some(hunk) = state.hunk.take() {
        finalize_hunk(
            hunk,
            &mut state,
            query,
            candidate,
            diff,
            index,
            target,
            target_hunk,
            target_line,
            total_hunks,
            total_lines,
            matches,
            anchors,
        )?;
    }
    validate_path(&state.path)?;
    let (before, after, context) = state.hashes.finish();
    let anchor = WorkbenchReviewAnchor::new(
        query.run(),
        query.query().workspace(),
        candidate,
        diff,
        state.path,
        before,
        after,
        context,
        WorkbenchReviewTarget::File,
        WorkbenchReviewRange::file(),
    )?;
    for (idx, requested) in anchors.iter().enumerate() {
        if !matches[idx] && *requested == anchor {
            matches[idx] = true;
        }
    }
    if index == target {
        if state.hunks == 0 && (target_hunk != 0 || target_line != 0) {
            return Err(malformed());
        }
        if state.hunks > 0 && target_hunk >= state.hunks {
            return Err(malformed());
        }
        *selected = Some((anchor, state.selected_hunk, state.hunks));
    }
    Ok(())
}

fn classify(line: &str) -> (WorkbenchDiffLineKind, &str) {
    match line.as_bytes().first().copied() {
        Some(b' ') => (WorkbenchDiffLineKind::Context, &line[1..]),
        Some(b'-') => (WorkbenchDiffLineKind::Removed, &line[1..]),
        Some(b'+') => (WorkbenchDiffLineKind::Added, &line[1..]),
        _ => (WorkbenchDiffLineKind::Metadata, line),
    }
}
fn hash_line(hashes: &mut Hashes, kind: WorkbenchDiffLineKind, text: &str) {
    match kind {
        WorkbenchDiffLineKind::Context => {
            hashes.before.update(text.as_bytes());
            hashes.before.update(b"\n");
            hashes.after.update(text.as_bytes());
            hashes.after.update(b"\n");
            hashes.context.update(text.as_bytes());
            hashes.context.update(b"\n");
        }
        WorkbenchDiffLineKind::Removed => {
            hashes.before.update(text.as_bytes());
            hashes.before.update(b"\n");
        }
        WorkbenchDiffLineKind::Added => {
            hashes.after.update(text.as_bytes());
            hashes.after.update(b"\n");
        }
        WorkbenchDiffLineKind::Metadata => {
            hashes.context.update(text.as_bytes());
            hashes.context.update(b"\n");
        }
    }
}
