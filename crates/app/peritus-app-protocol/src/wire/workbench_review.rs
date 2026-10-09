//! Canonical structured-review codecs with bounded collection allocation.

use super::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_DIFF_FILES, MAX_WORKBENCH_DIFF_HUNKS,
    MAX_WORKBENCH_DIFF_LINES, MAX_WORKBENCH_REVIEW_PAGE, WorkbenchDiffFile, WorkbenchDiffHunk,
    WorkbenchDiffLine, WorkbenchDiffLineKind, WorkbenchReviewAnchor, WorkbenchReviewComment,
    WorkbenchReviewCommentState, WorkbenchReviewEvidence, WorkbenchReviewEvidenceKind,
    WorkbenchReviewEvidenceState, WorkbenchReviewFeedback, WorkbenchReviewPage,
    WorkbenchReviewQuery, WorkbenchReviewRange, WorkbenchReviewTarget,
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

fn write_file(w: &mut CanonicalWriter, value: &WorkbenchDiffFile) -> Result<(), CodecError> {
    write_anchor(w, value.anchor())?;
    write_count(w, value.hunks().len())?;
    for hunk in value.hunks() {
        write_hunk(w, hunk)?;
    }
    Ok(())
}

fn read_file(r: &mut CanonicalReader<'_>) -> Result<WorkbenchDiffFile, CodecError> {
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
        lines.push(invalid(tag_offset, WorkbenchDiffLine::new(kind, r.read_str()?.to_owned()))?);
    }
    invalid(offset, WorkbenchDiffHunk::new(anchor, header, lines))
}

fn write_comment(
    w: &mut CanonicalWriter,
    value: &WorkbenchReviewComment,
) -> Result<(), CodecError> {
    write_id(w, value.id().as_bytes())?;
    w.write_u64(value.revision())?;
    write_anchor(w, value.anchor())?;
    w.write_u16(value.feedback().tag())?;
    w.write_str(value.message().as_str())?;
    super::workbench_inputs::write_selection(w, value.input())?;
    w.write_u16(value.state().tag())
}

fn read_comment(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReviewComment, CodecError> {
    let offset = r.offset();
    let id = read_id(r, ControlOperationId::new)?;
    let revision = r.read_u64()?;
    let anchor = read_anchor(r)?;
    let feedback = read_feedback(r)?;
    let message = super::workbench_inputs::read_text(r)?;
    let input = super::workbench_inputs::read_selection(r)?;
    let state_offset = r.offset();
    let state = WorkbenchReviewCommentState::from_tag(r.read_u16()?)
        .ok_or_else(|| CodecError::at(CodecErrorKind::UnknownTag, state_offset))?;
    invalid(
        offset,
        WorkbenchReviewComment::new(id, revision, anchor, feedback, message, input, state),
    )
}

fn write_evidence(
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

fn read_evidence(r: &mut CanonicalReader<'_>) -> Result<WorkbenchReviewEvidence, CodecError> {
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

fn write_count(w: &mut CanonicalWriter, count: usize) -> Result<(), CodecError> {
    w.write_u16(
        u16::try_from(count).map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )
}

fn read_count(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let offset = r.offset();
    let count = usize::from(r.read_u16()?);
    if count > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    Ok(count)
}
