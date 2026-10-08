//! Canonical bounded pages; each cursor retains its exact source scope.

use crate::{
    ConversationId, ImprovementCandidateSummary, ImprovementEvaluation, ImprovementEvidencePage,
    ImprovementEvidenceSummary, ImprovementPage, ImprovementPageCursor, ImprovementTextPage,
    ImprovementTextQuery, ImprovementTextReference, IMPROVEMENT_PAGE_ITEMS,
};
use crate::wire::primitive::{invalid, read_digest, read_id, write_digest, write_id};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{RunId, WorkspaceId};

pub(in crate::wire) fn write_cursor_option(w: &mut CanonicalWriter, value: Option<ImprovementPageCursor>) -> Result<(), CodecError> {
    w.write_option_tag(value.is_some())?;
    if let Some(v) = value {
        write_id(w, v.workspace().as_bytes())?;
        w.write_option_tag(v.candidate().is_some())?;
        if let Some(candidate) = v.candidate() { write_digest(w, candidate)?; }
        w.write_u64(v.revision())?;
        w.write_u64(v.highwater_sequence())?;
        w.write_u64(v.sequence())?;
    }
    Ok(())
}

pub(in crate::wire) fn read_cursor_option(r: &mut CanonicalReader<'_>) -> Result<Option<ImprovementPageCursor>, CodecError> {
    if !r.read_option_tag()? { return Ok(None); }
    let workspace = read_id(r, WorkspaceId::new)?;
    let candidate = if r.read_option_tag()? { Some(read_digest(r)?) } else { None };
    invalid(
        r.offset(),
        ImprovementPageCursor::snapshot(
            workspace,
            candidate,
            r.read_u64()?,
            r.read_u64()?,
            r.read_u64()?,
        ),
    )
    .map(Some)
}

fn write_reference(w: &mut CanonicalWriter, value: ImprovementTextReference) -> Result<(), CodecError> {
    write_digest(w, value.digest())?;
    w.write_u64(value.bytes())
}

fn read_reference(r: &mut CanonicalReader<'_>) -> Result<ImprovementTextReference, CodecError> {
    invalid(r.offset(), ImprovementTextReference::new(read_digest(r)?, r.read_u64()?))
}

fn write_evaluation(w: &mut CanonicalWriter, value: Option<ImprovementEvaluation>) -> Result<(), CodecError> {
    w.write_option_tag(value.is_some())?;
    if let Some(v) = value {
        write_id(w, v.conversation().as_bytes())?;
        write_id(w, v.run().as_bytes())?;
        write_id(w, v.target().as_bytes())?;
    }
    Ok(())
}

fn read_evaluation(r: &mut CanonicalReader<'_>) -> Result<Option<ImprovementEvaluation>, CodecError> {
    if !r.read_option_tag()? { return Ok(None); }
    Ok(Some(ImprovementEvaluation::new(read_id(r, ConversationId::new)?, read_id(r, RunId::new)?, read_id(r, WorkspaceId::new)?)))
}

pub(in crate::wire) fn write_page(w: &mut CanonicalWriter, value: &ImprovementPage) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    w.write_u64(value.revision())?;
    write_count(w, value.candidates().len())?;
    for v in value.candidates() {
        write_digest(w, v.id())?;
        write_reference(w, v.proposal())?;
        w.write_u64(v.evidence_count())?;
        write_evaluation(w, v.evaluation())?;
        w.write_bool(v.dismissed())?;
    }
    write_cursor_option(w, value.next())
}

pub(in crate::wire) fn read_page(r: &mut CanonicalReader<'_>) -> Result<ImprovementPage, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    let revision = r.read_u64()?;
    let count = read_count(r)?;
    let mut candidates = Vec::with_capacity(count);
    for _ in 0..count {
        candidates.push(ImprovementCandidateSummary::new(read_digest(r)?, read_reference(r)?, r.read_u64()?, read_evaluation(r)?, r.read_bool()?));
    }
    let next = read_cursor_option(r)?;
    invalid(r.offset(), ImprovementPage::new(workspace, revision, candidates, next))
}

pub(in crate::wire) fn write_evidence_page(w: &mut CanonicalWriter, value: &ImprovementEvidencePage) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    write_digest(w, value.candidate())?;
    w.write_u64(value.revision())?;
    write_count(w, value.evidence().len())?;
    for v in value.evidence() {
        write_id(w, v.run().as_bytes())?;
        write_reference(w, v.summary())?;
    }
    write_cursor_option(w, value.next())
}

pub(in crate::wire) fn read_evidence_page(r: &mut CanonicalReader<'_>) -> Result<ImprovementEvidencePage, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    let candidate = read_digest(r)?;
    let revision = r.read_u64()?;
    let count = read_count(r)?;
    let mut evidence = Vec::with_capacity(count);
    for _ in 0..count { evidence.push(ImprovementEvidenceSummary::new(read_id(r, RunId::new)?, read_reference(r)?)); }
    let next = read_cursor_option(r)?;
    invalid(r.offset(), ImprovementEvidencePage::new(workspace, candidate, revision, evidence, next))
}

pub(in crate::wire) fn write_query_body(w: &mut CanonicalWriter, value: ImprovementTextQuery) -> Result<(), CodecError> {
    write_digest(w, value.candidate())?;
    w.write_option_tag(value.run().is_some())?;
    if let Some(run) = value.run() { write_id(w, run.as_bytes())?; }
    write_reference(w, value.source())?;
    w.write_u64(value.offset())
}

pub(in crate::wire) fn read_query_body(r: &mut CanonicalReader<'_>, workspace: WorkspaceId) -> Result<ImprovementTextQuery, CodecError> {
    let candidate = read_digest(r)?;
    let run = if r.read_option_tag()? { Some(read_id(r, RunId::new)?) } else { None };
    invalid(
        r.offset(),
        ImprovementTextQuery::new(workspace, candidate, run, read_reference(r)?, r.read_u64()?),
    )
}

pub(in crate::wire) fn write_text_page(w: &mut CanonicalWriter, value: &ImprovementTextPage) -> Result<(), CodecError> {
    write_id(w, value.query().workspace().as_bytes())?;
    write_query_body(w, value.query())?;
    w.write_str(value.text())?;
    w.write_option_tag(value.next().is_some())?;
    if let Some(next) = value.next() { w.write_u64(next)?; }
    Ok(())
}

pub(in crate::wire) fn read_text_page(r: &mut CanonicalReader<'_>) -> Result<ImprovementTextPage, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    let query = read_query_body(r, workspace)?;
    let text = r.read_str()?.to_owned();
    let next = if r.read_option_tag()? { Some(r.read_u64()?) } else { None };
    invalid(r.offset(), ImprovementTextPage::new(query, text, next))
}

fn read_count(r: &mut CanonicalReader<'_>) -> Result<usize, CodecError> {
    let count = usize::from(r.read_u16()?);
    if count > IMPROVEMENT_PAGE_ITEMS { return Err(CodecError::at(CodecErrorKind::LimitExceeded, r.offset())); }
    Ok(count)
}

fn write_count(w: &mut CanonicalWriter, count: usize) -> Result<(), CodecError> {
    let count = u16::try_from(count).map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, w.len()))?;
    w.write_u16(count)
}
