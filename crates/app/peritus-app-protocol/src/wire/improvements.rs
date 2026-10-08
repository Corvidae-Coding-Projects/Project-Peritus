//! Bounded canonical candidate-inbox wire values.

use super::{
    primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id},
    product::{read_providers, write_providers},
};
use crate::{
    ImprovementCandidate, ImprovementEvaluation, ImprovementEvaluationRequest, ImprovementEvidence,
    ImprovementInbox, ImprovementRequest, ImprovementText,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{RunId, WorkspaceId};

mod paging;
pub(super) use paging::{read_page, read_evidence_page, read_text_page, write_page, write_evidence_page, write_text_page};

pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: &ImprovementRequest,
) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    match value {
        ImprovementRequest::List(_) => w.write_u16(1),
        ImprovementRequest::ListPage { after, .. } => {
            w.write_u16(5)?;
            paging::write_cursor_option(w, *after)
        }
        ImprovementRequest::EvidencePage { candidate, revision, after, .. } => {
            w.write_u16(6)?;
            write_digest(w, *candidate)?;
            w.write_u64(*revision)?;
            paging::write_cursor_option(w, *after)
        }
        ImprovementRequest::ReadText(query) => {
            w.write_u16(7)?;
            paging::write_query_body(w, *query)
        }
        ImprovementRequest::Suggest { run, proposal, .. } => {
            w.write_u16(2)?;
            write_id(w, run.as_bytes())?;
            w.write_str(proposal.as_str())
        }
        ImprovementRequest::Dismiss { candidate, .. } => {
            w.write_u16(3)?;
            write_digest(w, *candidate)
        }
        ImprovementRequest::Evaluate { candidate, evaluation, .. } => {
            w.write_u16(4)?;
            write_digest(w, *candidate)?;
            write_id(w, evaluation.run().as_bytes())?;
            write_id(w, evaluation.target().as_bytes())?;
            write_providers(w, evaluation.providers())
        }
    }
}

pub(super) fn read_request(r: &mut CanonicalReader<'_>) -> Result<ImprovementRequest, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    Ok(match r.read_u16()? {
        1 => ImprovementRequest::List(workspace),
        5 => ImprovementRequest::ListPage { workspace, after: paging::read_cursor_option(r)? },
        6 => ImprovementRequest::EvidencePage {
            workspace,
            candidate: read_digest(r)?,
            revision: r.read_u64()?,
            after: paging::read_cursor_option(r)?,
        },
        7 => ImprovementRequest::ReadText(paging::read_query_body(r, workspace)?),
        2 => ImprovementRequest::Suggest {
            workspace,
            run: read_id(r, RunId::new)?,
            proposal: read_text(r)?,
        },
        3 => ImprovementRequest::Dismiss { workspace, candidate: read_digest(r)? },
        4 => ImprovementRequest::Evaluate {
            workspace,
            candidate: read_digest(r)?,
            evaluation: ImprovementEvaluationRequest::new(
                read_id(r, RunId::new)?,
                read_id(r, WorkspaceId::new)?,
                read_providers(r)?,
            ),
        },
        _ => return unknown(r.offset()),
    })
}

pub(super) fn write_inbox(
    w: &mut CanonicalWriter,
    value: &ImprovementInbox,
) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    w.write_u16(
        u16::try_from(value.candidates().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for item in value.candidates() {
        write_digest(w, item.id())?;
        w.write_str(item.proposal().as_str())?;
        w.write_bool(item.dismissed())?;
        w.write_option_tag(item.evaluation().is_some())?;
        if let Some(evaluation) = item.evaluation() {
            write_id(w, evaluation.conversation().as_bytes())?;
            write_id(w, evaluation.run().as_bytes())?;
            write_id(w, evaluation.target().as_bytes())?;
        }
        w.write_u16(
            u16::try_from(item.evidence().len())
                .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
        )?;
        for evidence in item.evidence() {
            write_id(w, evidence.run().as_bytes())?;
            write_digest(w, evidence.digest())?;
            w.write_str(evidence.summary().as_str())?;
        }
    }
    Ok(())
}

pub(super) fn read_inbox(r: &mut CanonicalReader<'_>) -> Result<ImprovementInbox, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    let count = read_count(r)?;
    let mut candidates = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_digest(r)?;
        let proposal = read_text(r)?;
        let dismissed = r.read_bool()?;
        let evaluation = if r.read_option_tag()? {
            Some(ImprovementEvaluation::new(
                read_id(r, crate::ConversationId::new)?,
                read_id(r, RunId::new)?,
                read_id(r, WorkspaceId::new)?,
            ))
        } else {
            None
        };
        let count = read_count(r)?;
        let mut evidence = Vec::with_capacity(count);
        for _ in 0..count {
            evidence.push(ImprovementEvidence::new(
                read_id(r, RunId::new)?,
                read_digest(r)?,
                read_text(r)?,
            ));
        }
        candidates.push(invalid(
            r.offset(),
            ImprovementCandidate::new(id, proposal, evidence, evaluation, dismissed),
        )?);
    }
    invalid(r.offset(), ImprovementInbox::new(workspace, candidates))
}

fn read_count(r: &mut CanonicalReader<'_>) -> Result<usize, CodecError> {
    r.read_u16().map(usize::from)
}

fn read_text(r: &mut CanonicalReader<'_>) -> Result<ImprovementText, CodecError> {
    let text = r.read_str()?;
    invalid(r.offset(), ImprovementText::new(text.to_owned()))
}
