//! Bounded diff-page content, comment, and evidence codecs.

use super::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, ControlOperationId,
    MAX_WORKBENCH_DIFF_HUNKS, MAX_WORKBENCH_DIFF_LINES, WorkbenchDiffFile, WorkbenchDiffHunk,
    WorkbenchDiffLine, WorkbenchDiffLineKind, WorkbenchReviewComment, WorkbenchReviewCommentState,
    WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind, WorkbenchReviewEvidenceState, invalid,
    read_anchor, read_digest, read_feedback, read_id, write_anchor, write_digest, write_id,
};

pub(super) fn write_file(
    w: &mut CanonicalWriter,
    value: &WorkbenchDiffFile,
) -> Result<(), CodecError> {
    write_anchor(w, value.anchor())?;
    write_count(w, value.hunks().len())?;
    for hunk in value.hunks() {
        write_hunk(w, hunk)?;
    }
    Ok(())
}

pub(super) fn read_file(r: &mut CanonicalReader<'_>) -> Result<WorkbenchDiffFile, CodecError> {
    let offset = r.offset();
    let anchor = read_anchor(r)?;
    let count = read_count(r, MAX_WORKBENCH_DIFF_HUNKS)?;
    let mut hunks = Vec::with_capacity(count);
    for _ in 0..count {
        hunks.push(read_hunk(r)?);
    }
    invalid(offset, WorkbenchDiffFile::new(anchor, hunks))
}

fn write_hunk(w: &mut CanonicalWriter, value: &WorkbenchDiffHunk) -> Result<(), CodecError> {
    write_anchor(w, value.anchor())?;
    w.write_str(value.header())?;
    write_count(w, value.lines().len())?;
    for line in value.lines() {
        w.write_u16(line.kind().tag())?;
        w.write_str(line.text())?;
    }
    Ok(())
}

fn read_hunk(r: &mut CanonicalReader<'_>) -> Result<WorkbenchDiffHunk, CodecError> {
    let offset = r.offset();
    let anchor = read_anchor(r)?;
    let header = r.read_str()?.to_owned();
    let count = read_count(r, MAX_WORKBENCH_DIFF_LINES)?;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let tag_offset = r.offset();
        let kind = WorkbenchDiffLineKind::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, tag_offset))?;
        lines.push(invalid(tag_offset, WorkbenchDiffLine::new(kind, r.read_str()?))?);
    }
    invalid(offset, WorkbenchDiffHunk::new(anchor, header, lines))
}

pub(super) fn write_diff_page_hunk(
    w: &mut CanonicalWriter,
    value: &WorkbenchDiffHunk,
) -> Result<(), CodecError> {
    write_anchor(w, value.anchor())?;
    w.write_str(value.header())?;
    write_count(w, value.lines().len())?;
    for line in value.lines() {
        w.write_u16(line.kind().tag())?;
        w.write_str(line.text())?;
        w.write_u32(line.raw_offset())?;
        w.write_u32(line.raw_length())?;
        w.write_bool(line.is_truncated())?;
    }
    Ok(())
}

pub(super) fn read_diff_page_hunk(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchDiffHunk, CodecError> {
    let offset = r.offset();
    let anchor = read_anchor(r)?;
    let header = r.read_str()?.to_owned();
    let count = read_count(r, crate::MAX_WORKBENCH_DIFF_PAGE_LINES)?;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let item_offset = r.offset();
        let kind = WorkbenchDiffLineKind::from_tag(r.read_u16()?)
            .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, item_offset))?;
        let text = r.read_str()?.to_owned();
        let raw_offset = r.read_u32()?;
        let raw_length = r.read_u32()?;
        let truncated = r.read_bool()?;
        lines.push(invalid(
            item_offset,
            WorkbenchDiffLine::from_wire(kind, text, raw_offset, raw_length, truncated),
        )?);
    }
    invalid(offset, WorkbenchDiffHunk::new(anchor, header, lines))
}

pub(super) fn write_comment(
    w: &mut CanonicalWriter,
    value: &WorkbenchReviewComment,
) -> Result<(), CodecError> {
    write_id(w, value.id().as_bytes())?;
    w.write_u64(value.revision())?;
    write_anchor(w, value.anchor())?;
    w.write_u16(value.feedback().tag())?;
    w.write_str(value.message().as_str())?;
    super::super::workbench_inputs::write_selection(w, value.input())?;
    w.write_u16(value.state().tag())
}

pub(super) fn read_comment(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchReviewComment, CodecError> {
    let offset = r.offset();
    let id = read_id(r, ControlOperationId::new)?;
    let revision = r.read_u64()?;
    let anchor = read_anchor(r)?;
    let feedback = read_feedback(r)?;
    let message = super::super::workbench_inputs::read_text(r)?;
    let input = super::super::workbench_inputs::read_selection(r)?;
    let state_offset = r.offset();
    let state = WorkbenchReviewCommentState::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, state_offset))?;
    invalid(
        offset,
        WorkbenchReviewComment::new(id, revision, anchor, feedback, message, input, state),
    )
}

pub(super) fn write_evidence(
    w: &mut CanonicalWriter,
    value: WorkbenchReviewEvidence,
) -> Result<(), CodecError> {
    w.write_u16(value.kind().tag())?;
    w.write_u16(value.state().tag())?;
    w.write_bool(value.candidate_digest().is_some())?;
    if let (Some(candidate), Some(revision), Some(sequence)) =
        (value.candidate_digest(), value.conversation_revision(), value.checkpoint_sequence())
    {
        write_digest(w, candidate)?;
        w.write_u64(revision)?;
        w.write_u64(sequence)?;
    }
    Ok(())
}

pub(super) fn read_evidence(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchReviewEvidence, CodecError> {
    let offset = r.offset();
    let kind = WorkbenchReviewEvidenceKind::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let state = WorkbenchReviewEvidenceState::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, offset))?;
    let (candidate, revision, sequence) = if r.read_bool()? {
        (Some(read_digest(r)?), Some(r.read_u64()?), Some(r.read_u64()?))
    } else {
        (None, None, None)
    };
    invalid(offset, WorkbenchReviewEvidence::new(kind, state, candidate, revision, sequence))
}

pub(super) fn write_count(w: &mut CanonicalWriter, count: usize) -> Result<(), CodecError> {
    w.write_u16(
        u16::try_from(count).map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )
}

pub(super) fn read_count(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let offset = r.offset();
    let count = usize::from(r.read_u16()?);
    if count > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(count)
}
