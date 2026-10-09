//! Deterministic parsing of retained unified diffs into exact review targets.

use super::{
    WorkbenchDiffFile, WorkbenchDiffHunk, WorkbenchDiffLine, WorkbenchDiffLineKind,
    WorkbenchReviewAnchor, WorkbenchReviewDiffPage, WorkbenchReviewDiffQuery, WorkbenchReviewRange,
    WorkbenchReviewTarget, malformed, validate_path,
};
use crate::AppProtocolError;
use peritus_codec::sha256;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

/// Parses and projects one structured page with bounded retained content.
///
/// # Errors
/// Rejects malformed diff sections and cursors outside the exact retained diff.
pub fn parse_workbench_diff_page(
    query: WorkbenchReviewDiffQuery,
    candidate_digest: Sha256Digest,
    raw: &str,
) -> Result<WorkbenchReviewDiffPage, AppProtocolError> {
    super::page_parser::parse_page(query, candidate_digest, raw, &[]).map(|(page, _)| page)
}

/// Parses one bounded page and tests exact file/hunk anchors during the same streaming pass.
///
/// # Errors
/// Rejects malformed diff sections, unsafe paths, invalid hunk metadata, and
/// cursors outside the exact retained diff.
pub fn parse_workbench_diff_page_with_anchors(
    query: WorkbenchReviewDiffQuery,
    candidate_digest: Sha256Digest,
    raw: &str,
    anchors: &[WorkbenchReviewAnchor],
) -> Result<(WorkbenchReviewDiffPage, Vec<bool>), AppProtocolError> {
    super::page_parser::parse_page(query, candidate_digest, raw, anchors)
}

/// Deterministically parses one retained unified diff into content-bound feedback targets.
/// Metadata before the first `diff --git` section remains available in the raw toggle only.
///
/// # Errors
/// Rejects malformed hunks, unsafe paths, or collection/line limits. Raw inspection remains valid.
pub fn parse_workbench_diff(
    run: RunId,
    workspace: WorkspaceId,
    candidate_digest: Sha256Digest,
    raw: &str,
) -> Result<(Sha256Digest, Vec<WorkbenchDiffFile>), AppProtocolError> {
    let diff_digest = sha256(raw.as_bytes());
    let mut files = Vec::new();
    let mut current: Option<FileBuilder> = None;
    let mut source_offset = 0_usize;
    for raw_line in raw.split_inclusive('\n') {
        let raw_length = raw_line.len();
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(header) = line.strip_prefix("diff --git ") {
            if let Some(file) = current.take() {
                files.push(file.finish(run, workspace, candidate_digest, diff_digest)?);
            }
            current = Some(FileBuilder::new(path_from_header(header)?));
            source_offset = source_offset.saturating_add(raw_length);
            continue;
        }
        let Some(file) = current.as_mut() else {
            source_offset = source_offset.saturating_add(raw_length);
            continue;
        };
        if file.hunk.is_none() {
            if let Some(path) = line.strip_prefix("+++ ").and_then(diff_path) {
                file.path = path;
            } else if file.path.is_empty()
                && let Some(path) = line.strip_prefix("--- ").and_then(diff_path)
            {
                file.path = path;
            }
        }
        if line.starts_with("@@ ") {
            file.start_hunk(line)?;
        } else if file.hunk.is_some() {
            file.push_line(line, source_offset, raw_length)?;
        } else {
            file.section.extend_from_slice(line.as_bytes());
            file.section.push(b'\n');
        }
        source_offset = source_offset.saturating_add(raw_length);
    }
    if let Some(file) = current {
        files.push(file.finish(run, workspace, candidate_digest, diff_digest)?);
    }
    Ok((diff_digest, files))
}

struct FileBuilder {
    path: String,
    section: Vec<u8>,
    hunks: Vec<HunkBuilder>,
    hunk: Option<HunkBuilder>,
}

impl FileBuilder {
    const fn new(path: String) -> Self {
        Self { path, section: Vec::new(), hunks: Vec::new(), hunk: None }
    }
    fn start_hunk(&mut self, header: &str) -> Result<(), AppProtocolError> {
        if let Some(hunk) = self.hunk.take() {
            self.hunks.push(hunk);
        }
        self.hunk = Some(HunkBuilder::new(header)?);
        Ok(())
    }
    fn push_line(
        &mut self,
        line: &str,
        raw_offset: usize,
        raw_length: usize,
    ) -> Result<(), AppProtocolError> {
        self.hunk.as_mut().ok_or_else(malformed)?.push(line, raw_offset, raw_length)
    }
    fn finish(
        mut self,
        run: RunId,
        workspace: WorkspaceId,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
    ) -> Result<WorkbenchDiffFile, AppProtocolError> {
        if let Some(hunk) = self.hunk.take() {
            self.hunks.push(hunk);
        }
        validate_path(&self.path)?;
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut context = self.section;
        let hunks = self
            .hunks
            .into_iter()
            .map(|hunk| {
                before.extend_from_slice(&hunk.before);
                after.extend_from_slice(&hunk.after);
                context.extend_from_slice(&hunk.context);
                hunk.finish(run, workspace, candidate_digest, diff_digest, &self.path)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let anchor = WorkbenchReviewAnchor::new(
            run,
            workspace,
            candidate_digest,
            diff_digest,
            self.path,
            sha256(&before),
            sha256(&after),
            sha256(&context),
            WorkbenchReviewTarget::File,
            WorkbenchReviewRange::file(),
        )?;
        WorkbenchDiffFile::new(anchor, hunks)
    }
}

struct HunkBuilder {
    header: String,
    range: WorkbenchReviewRange,
    lines: Vec<WorkbenchDiffLine>,
    before: Vec<u8>,
    after: Vec<u8>,
    context: Vec<u8>,
}

impl HunkBuilder {
    fn new(header: &str) -> Result<Self, AppProtocolError> {
        let range = parse_hunk_range(header)?;
        Ok(Self {
            header: header.to_owned(),
            range,
            lines: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            context: header.as_bytes().to_vec(),
        })
    }
    fn push(
        &mut self,
        line: &str,
        raw_offset: usize,
        raw_length: usize,
    ) -> Result<(), AppProtocolError> {
        let (kind, text) = match line.as_bytes().first().copied() {
            Some(b' ') => (WorkbenchDiffLineKind::Context, &line[1..]),
            Some(b'-') => (WorkbenchDiffLineKind::Removed, &line[1..]),
            Some(b'+') => (WorkbenchDiffLineKind::Added, &line[1..]),
            _ => (WorkbenchDiffLineKind::Metadata, line),
        };
        let append = |target: &mut Vec<u8>, text: &str| {
            target.extend_from_slice(text.as_bytes());
            target.push(b'\n');
        };
        match kind {
            WorkbenchDiffLineKind::Context => {
                append(&mut self.before, text);
                append(&mut self.after, text);
                append(&mut self.context, text);
            }
            WorkbenchDiffLineKind::Removed => append(&mut self.before, text),
            WorkbenchDiffLineKind::Added => append(&mut self.after, text),
            WorkbenchDiffLineKind::Metadata => append(&mut self.context, text),
        }
        let prefix = usize::from(matches!(
            kind,
            WorkbenchDiffLineKind::Context
                | WorkbenchDiffLineKind::Added
                | WorkbenchDiffLineKind::Removed
        ));
        let source_offset = raw_offset.saturating_add(prefix);
        let _source_length = raw_length.saturating_sub(prefix).min(text.len());
        self.lines.push(WorkbenchDiffLine::from_source(kind, text, source_offset)?);
        Ok(())
    }
    fn finish(
        self,
        run: RunId,
        workspace: WorkspaceId,
        candidate_digest: Sha256Digest,
        diff_digest: Sha256Digest,
        path: &str,
    ) -> Result<WorkbenchDiffHunk, AppProtocolError> {
        let anchor = WorkbenchReviewAnchor::new(
            run,
            workspace,
            candidate_digest,
            diff_digest,
            path.to_owned(),
            sha256(&self.before),
            sha256(&self.after),
            sha256(&self.context),
            WorkbenchReviewTarget::Hunk,
            self.range,
        )?;
        WorkbenchDiffHunk::new(anchor, self.header, self.lines)
    }
}

pub(super) fn parse_hunk_range(header: &str) -> Result<WorkbenchReviewRange, AppProtocolError> {
    let body = header
        .strip_prefix("@@ ")
        .and_then(|value| value.split_once(" @@"))
        .map(|(value, _)| value)
        .ok_or_else(malformed)?;
    let mut fields = body.split_whitespace();
    let old = fields.next().and_then(|value| value.strip_prefix('-')).ok_or_else(malformed)?;
    let new = fields.next().and_then(|value| value.strip_prefix('+')).ok_or_else(malformed)?;
    if fields.next().is_some() {
        return Err(malformed());
    }
    let (old_start, old_lines) = parse_range(old)?;
    let (new_start, new_lines) = parse_range(new)?;
    WorkbenchReviewRange::hunk(old_start, old_lines, new_start, new_lines)
}

fn parse_range(value: &str) -> Result<(u32, u32), AppProtocolError> {
    let (start, count) = value.split_once(',').map_or((value, "1"), |pair| pair);
    Ok((start.parse().map_err(|_| malformed())?, count.parse().map_err(|_| malformed())?))
}

pub(super) fn path_from_header(header: &str) -> Result<String, AppProtocolError> {
    let (source, remainder) = take_path_token(header)?;
    let (target, remainder) = take_path_token(remainder.trim_start())?;
    if !remainder.trim().is_empty() {
        return Err(malformed());
    }
    let source = decode_git_path(source)?;
    let target = decode_git_path(target)?;
    if !source.starts_with("a/") || !target.starts_with("b/") {
        return Err(malformed());
    }
    let path = target[2..].to_owned();
    if path.is_empty() { Err(malformed()) } else { Ok(path) }
}

pub(super) fn diff_path(value: &str) -> Option<String> {
    let value = value.trim();
    let value = if value.starts_with('"') {
        let (token, _) = take_path_token(value).ok()?;
        decode_git_path(token).ok()?
    } else {
        value.split_once('\t').map_or(value, |(path, _)| path).to_owned()
    };
    if value == "/dev/null" {
        return None;
    }
    value.strip_prefix("a/").or_else(|| value.strip_prefix("b/")).map(str::to_owned)
}

fn take_path_token(value: &str) -> Result<(&str, &str), AppProtocolError> {
    let bytes = value.as_bytes();
    if bytes.first() == Some(&b'"') {
        let mut cursor = 1_usize;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' => cursor = cursor.checked_add(2).ok_or_else(malformed)?,
                b'"' => {
                    let end = cursor + 1;
                    return Ok((&value[..end], &value[end..]));
                }
                _ => cursor += 1,
            }
        }
        Err(malformed())
    } else {
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        if end == 0 { Err(malformed()) } else { Ok((&value[..end], &value[end..])) }
    }
}

fn decode_git_path(token: &str) -> Result<String, AppProtocolError> {
    if !token.starts_with('"') {
        return Ok(token.to_owned());
    }
    if !token.ends_with('"') || token.len() < 2 {
        return Err(malformed());
    }
    let bytes = token.as_bytes();
    let mut decoded = Vec::with_capacity(token.len().saturating_sub(2));
    let mut cursor = 1_usize;
    while cursor < bytes.len() - 1 {
        if bytes[cursor] != b'\\' {
            decoded.push(bytes[cursor]);
            cursor += 1;
            continue;
        }
        cursor += 1;
        let escaped = *bytes.get(cursor).ok_or_else(malformed)?;
        if matches!(escaped, b'0'..=b'7') {
            let mut value = u16::from(escaped - b'0');
            cursor += 1;
            for _ in 1..3 {
                let Some(digit @ b'0'..=b'7') = bytes.get(cursor).copied() else { break };
                value = value * 8 + u16::from(digit - b'0');
                cursor += 1;
            }
            decoded.push(u8::try_from(value).map_err(|_| malformed())?);
            continue;
        }
        decoded.push(match escaped {
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            b'\\' => b'\\',
            b'"' => b'"',
            _ => return Err(malformed()),
        });
        cursor += 1;
    }
    String::from_utf8(decoded).map_err(|_| malformed())
}
