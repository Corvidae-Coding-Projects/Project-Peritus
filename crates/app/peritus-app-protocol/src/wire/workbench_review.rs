//! Canonical structured-review codecs with bounded collection allocation.

mod content;

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_DIFF_FILES, MAX_WORKBENCH_DIFF_HUNKS,
    MAX_WORKBENCH_DIFF_LINES, MAX_WORKBENCH_REVIEW_PAGE, WorkbenchDiffFile, WorkbenchDiffHunk,
    WorkbenchDiffLine, WorkbenchDiffLineKind, WorkbenchReviewAnchor, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery, WorkbenchReviewRange, WorkbenchReviewTarget,
};
use content::{
    read_comment, read_count, read_diff_page_hunk, read_evidence, read_file, write_comment,
    write_count, write_diff_page_hunk, write_evidence, write_file,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{RunId, WorkspaceId};

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchReviewQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    write_id(w, value.run().as_bytes())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.offset())
}

pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReviewQuery, CodecError> {
    Ok(WorkbenchReviewQuery::new(
        super::workbench::read_query(r)?,
        read_id(r, RunId::new)?,
        r.read_u64()?,
        r.read_u32()?,
    ))
}

pub(super) fn write_diff_query(
    w: &mut CanonicalWriter,
    value: crate::WorkbenchReviewDiffQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    write_id(w, value.run().as_bytes())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.file_offset())?;
    w.write_u32(value.hunk_offset())?;
    w.write_u32(value.line_offset())
}

pub(super) fn read_diff_query(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchReviewDiffQuery, CodecError> {
    Ok(crate::WorkbenchReviewDiffQuery::new(
        super::workbench::read_query(r)?,
        read_id(r, RunId::new)?,
        r.read_u64()?,
        r.read_u32()?,
        r.read_u32()?,
        r.read_u32()?,
    ))
}

pub(super) fn write_diff_bytes_query(
    w: &mut CanonicalWriter,
    value: crate::WorkbenchReviewDiffBytesQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    write_id(w, value.run().as_bytes())?;
    w.write_u64(value.revision())?;
    write_digest(w, value.candidate_digest())?;
    write_digest(w, value.diff_digest())?;
    w.write_u32(value.offset())?;
    w.write_u32(value.maximum_bytes())
}

pub(super) fn read_diff_bytes_query(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchReviewDiffBytesQuery, CodecError> {
    Ok(crate::WorkbenchReviewDiffBytesQuery::new(
        super::workbench::read_query(r)?,
        read_id(r, RunId::new)?,
        r.read_u64()?,
        read_digest(r)?,
        read_digest(r)?,
        r.read_u32()?,
        r.read_u32()?,
    ))
}

pub(super) fn write_diff_bytes(
    w: &mut CanonicalWriter,
    value: &crate::WorkbenchReviewDiffBytes,
) -> Result<(), CodecError> {
    write_diff_bytes_query(w, value.query())?;
    w.write_u32(value.total_bytes())?;
    w.write_bytes(value.bytes())
}

pub(super) fn read_diff_bytes(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchReviewDiffBytes, CodecError> {
    let offset = r.offset();
    let query = read_diff_bytes_query(r)?;
    let total = r.read_u32()?;
    let bytes = r.read_bytes()?.to_vec();
    invalid(offset, crate::WorkbenchReviewDiffBytes::new(query, total, bytes))
}

pub(super) fn write_diff_page(
    w: &mut CanonicalWriter,
    value: &crate::WorkbenchReviewDiffPage,
) -> Result<(), CodecError> {
    write_diff_query(w, value.query())?;
    write_digest(w, value.candidate_digest())?;
    write_digest(w, value.diff_digest())?;
    write_anchor(w, value.file_anchor())?;
    w.write_bool(value.hunk().is_some())?;
    if let Some(hunk) = value.hunk() {
        write_diff_page_hunk(w, hunk)?;
    }
    write_count(w, value.lines().len())?;
    for line in value.lines() {
        w.write_u16(line.kind().tag())?;
        w.write_str(line.preview())?;
        w.write_u32(line.raw_offset())?;
        w.write_u32(line.raw_length())?;
        w.write_bool(line.is_truncated())?;
    }
    w.write_u32(value.total_files())?;
    w.write_u64(value.total_hunks())?;
    w.write_u64(value.total_lines())?;
    w.write_bool(value.next().is_some())?;
    if let Some(next) = value.next() {
        write_diff_query(w, next)?;
    }
    Ok(())
}

pub(super) fn read_diff_page(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchReviewDiffPage, CodecError> {
    let offset = r.offset();
    let query = read_diff_query(r)?;
    let candidate = read_digest(r)?;
    let diff = read_digest(r)?;
    let file = read_anchor(r)?;
    let hunk = if r.read_bool()? { Some(read_diff_page_hunk(r)?) } else { None };
    let count = read_count(r, crate::MAX_WORKBENCH_DIFF_PAGE_LINES)?;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let item_offset = r.offset();
        let kind = WorkbenchDiffLineKind::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, item_offset))?;
        let preview = r.read_str()?.to_owned();
        let raw_offset = r.read_u32()?;
        let raw_length = r.read_u32()?;
        let truncated = r.read_bool()?;
        lines.push(invalid(
            item_offset,
            crate::WorkbenchReviewDiffLine::from_wire(
                kind, preview, raw_offset, raw_length, truncated,
            ),
        )?);
    }
    let total_files = r.read_u32()?;
    let total_hunks = r.read_u64()?;
    let total_lines = r.read_u64()?;
    let next = if r.read_bool()? { Some(read_diff_query(r)?) } else { None };
    invalid(
        offset,
        crate::WorkbenchReviewDiffPage::from_wire_parts(
            query,
            candidate,
            diff,
            file,
            hunk,
            lines,
            total_files,
            total_hunks,
            total_lines,
            next,
        ),
    )
}

pub(super) fn write_anchor(
    w: &mut CanonicalWriter,
    value: &WorkbenchReviewAnchor,
) -> Result<(), CodecError> {
    write_id(w, value.run().as_bytes())?;
    write_id(w, value.workspace().as_bytes())?;
    for digest in [
        value.candidate_digest(),
        value.diff_digest(),
        value.before_blob_digest(),
        value.after_blob_digest(),
        value.context_digest(),
    ] {
        write_digest(w, digest)?;
    }
    w.write_str(value.path())?;
    w.write_u16(value.target().tag())?;
    let range = value.range();
    w.write_u32(range.old_start())?;
    w.write_u32(range.old_lines())?;
    w.write_u32(range.new_start())?;
    w.write_u32(range.new_lines())
}

pub(super) fn read_anchor(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchReviewAnchor, CodecError> {
    let offset = r.offset();
    let run = read_id(r, RunId::new)?;
    let workspace = read_id(r, WorkspaceId::new)?;
    let candidate = read_digest(r)?;
    let diff = read_digest(r)?;
    let before = read_digest(r)?;
    let after = read_digest(r)?;
    let context = read_digest(r)?;
    let path = r.read_str()?.to_owned();
    let target = read_target(r)?;
    let old_start = r.read_u32()?;
    let old_lines = r.read_u32()?;
    let new_start = r.read_u32()?;
    let new_lines = r.read_u32()?;
    let range = match target {
        WorkbenchReviewTarget::File => WorkbenchReviewRange::file(),
        WorkbenchReviewTarget::Hunk => {
            invalid(offset, WorkbenchReviewRange::hunk(old_start, old_lines, new_start, new_lines))?
        }
    };
    if target == WorkbenchReviewTarget::File
        && (old_start, old_lines, new_start, new_lines) != (0, 0, 0, 0)
    {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    invalid(
        offset,
        WorkbenchReviewAnchor::new(
            run, workspace, candidate, diff, path, before, after, context, target, range,
        ),
    )
}

fn read_target(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReviewTarget, CodecError> {
    let offset = r.offset();
    WorkbenchReviewTarget::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))
}

pub(super) fn read_feedback(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchReviewFeedback, CodecError> {
    let offset = r.offset();
    WorkbenchReviewFeedback::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))
}

pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchReviewPage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    write_digest(w, value.candidate_digest())?;
    write_digest(w, value.diff_digest())?;
    write_count(w, value.files().len())?;
    for file in value.files() {
        write_file(w, file)?;
    }
    write_count(w, value.comments().len())?;
    for comment in value.comments() {
        write_comment(w, comment)?;
    }
    w.write_u32(value.total_comments())?;
    write_count(w, value.evidence().len())?;
    for evidence in value.evidence() {
        write_evidence(w, *evidence)?;
    }
    Ok(())
}

pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReviewPage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let candidate = read_digest(r)?;
    let diff = read_digest(r)?;
    let file_count = read_count(r, MAX_WORKBENCH_DIFF_FILES)?;
    let mut files = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        files.push(read_file(r)?);
    }
    let comment_count = read_count(r, MAX_WORKBENCH_REVIEW_PAGE)?;
    let mut comments = Vec::with_capacity(comment_count);
    for _ in 0..comment_count {
        comments.push(read_comment(r)?);
    }
    let total = r.read_u32()?;
    let evidence_count = read_count(r, 2)?;
    let mut evidence = Vec::with_capacity(evidence_count);
    for _ in 0..evidence_count {
        evidence.push(read_evidence(r)?);
    }
    invalid(
        offset,
        WorkbenchReviewPage::new(query, candidate, diff, files, comments, total, evidence),
    )
}

pub(super) fn write_summary(
    w: &mut CanonicalWriter,
    value: &crate::WorkbenchReviewSummary,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    write_digest(w, value.candidate_digest())?;
    write_digest(w, value.diff_digest())?;
    w.write_u32(value.total_files())?;
    w.write_u64(value.total_hunks())?;
    w.write_u64(value.total_lines())?;
    w.write_bool(value.structured_available())?;
    write_count(w, value.comments().len())?;
    for comment in value.comments() {
        write_comment(w, comment)?;
    }
    w.write_u32(value.total_comments())?;
    write_count(w, value.evidence().len())?;
    for evidence in value.evidence() {
        write_evidence(w, *evidence)?;
    }
    Ok(())
}

pub(super) fn read_summary(
    r: &mut CanonicalReader<'_>,
) -> Result<crate::WorkbenchReviewSummary, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let candidate = read_digest(r)?;
    let diff = read_digest(r)?;
    let files = r.read_u32()?;
    let hunks = r.read_u64()?;
    let lines = r.read_u64()?;
    let available = r.read_bool()?;
    let count = read_count(r, MAX_WORKBENCH_REVIEW_PAGE)?;
    let mut comments = Vec::with_capacity(count);
    for _ in 0..count {
        comments.push(read_comment(r)?);
    }
    let total = r.read_u32()?;
    let evidence_count = read_count(r, 2)?;
    let mut evidence = Vec::with_capacity(evidence_count);
    for _ in 0..evidence_count {
        evidence.push(read_evidence(r)?);
    }
    invalid(
        offset,
        crate::WorkbenchReviewSummary::new(
            query, candidate, diff, files, hunks, lines, available, comments, total, evidence,
        ),
    )
}
