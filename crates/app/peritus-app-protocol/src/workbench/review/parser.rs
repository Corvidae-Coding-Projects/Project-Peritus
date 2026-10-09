//! Deterministic parsing of retained unified diffs into exact review targets.

use super::{
    MAX_WORKBENCH_DIFF_FILES, MAX_WORKBENCH_DIFF_HUNKS, MAX_WORKBENCH_DIFF_LINES,
    WorkbenchDiffFile, WorkbenchDiffHunk, WorkbenchDiffLine, WorkbenchDiffLineKind,
    WorkbenchReviewAnchor, WorkbenchReviewRange, WorkbenchReviewTarget, limit, malformed,
    validate_path,
};
use crate::AppProtocolError;
use peritus_codec::sha256;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

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
    let mut total_hunks = 0_usize;
    let mut total_lines = 0_usize;

    for line in raw.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            if let Some(file) = current.take() {
                files.push(file.finish(run, workspace, candidate_digest, diff_digest)?);
            }
            if files.len() >= MAX_WORKBENCH_DIFF_FILES {
                return Err(limit());
            }
            current = Some(FileBuilder::new(path_from_header(header)?));
            continue;
        }
        let Some(file) = current.as_mut() else { continue };
        if let Some(path) = line.strip_prefix("+++ ").and_then(diff_path) {
            file.path = path;
        } else if file.path.is_empty()
            && let Some(path) = line.strip_prefix("--- ").and_then(diff_path)
        {
            file.path = path;
        }
        if line.starts_with("@@ ") {
            total_hunks = total_hunks.checked_add(1).ok_or_else(limit)?;
            if total_hunks > MAX_WORKBENCH_DIFF_HUNKS {
                return Err(limit());
            }
            file.start_hunk(line)?;
        } else if file.hunk.is_some() {
            total_lines = total_lines.checked_add(1).ok_or_else(limit)?;
            if total_lines > MAX_WORKBENCH_DIFF_LINES {
                return Err(limit());
            }
            file.push_line(line)?;
        } else {
            file.section.extend_from_slice(line.as_bytes());
            file.section.push(b'\n');
        }
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
    fn push_line(&mut self, line: &str) -> Result<(), AppProtocolError> {
        self.hunk.as_mut().ok_or_else(malformed)?.push(line)
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
    fn push(&mut self, line: &str) -> Result<(), AppProtocolError> {
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
        self.lines.push(WorkbenchDiffLine::new(kind, text.to_owned())?);
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

fn parse_hunk_range(header: &str) -> Result<WorkbenchReviewRange, AppProtocolError> {
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

fn path_from_header(header: &str) -> Result<String, AppProtocolError> {
    header
        .rfind(" b/")
        .map(|offset| header[offset + 3..].to_owned())
        .filter(|path| !path.is_empty())
        .ok_or_else(malformed)
}

fn diff_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value == "/dev/null" {
        return None;
    }
    value.strip_prefix("a/").or_else(|| value.strip_prefix("b/")).map(str::to_owned)
}
