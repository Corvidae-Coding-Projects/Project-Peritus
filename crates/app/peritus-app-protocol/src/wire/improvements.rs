//! Bounded canonical candidate-inbox wire values.

use super::{
    primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id},
    product::{read_run_request, write_run_request},
};
use crate::{
    ImprovementCandidate, ImprovementEvidence, ImprovementInbox, ImprovementRequest,
    ImprovementText, MAX_IMPROVEMENT_EVIDENCE, MAX_IMPROVEMENTS,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::{RunId, WorkspaceId};

pub(super) fn write_request(
    w: &mut CanonicalWriter,
    value: &ImprovementRequest,
) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    match value {
        ImprovementRequest::List(_) => w.write_u16(1),
        ImprovementRequest::Suggest { run, proposal, .. } => {
            w.write_u16(2)?;
            write_id(w, run.as_bytes())?;
            w.write_str(proposal.as_str())
        }
        ImprovementRequest::Dismiss { candidate, .. } => {
            w.write_u16(3)?;
            write_digest(w, *candidate)
        }
        ImprovementRequest::Evaluate { candidate, run, .. } => {
            w.write_u16(4)?;
            write_digest(w, *candidate)?;
            write_run_request(w, run)
        }
    }
}

pub(super) fn read_request(r: &mut CanonicalReader<'_>) -> Result<ImprovementRequest, CodecError> {
    let workspace = read_id(r, WorkspaceId::new)?;
    Ok(match r.read_u16()? {
        1 => ImprovementRequest::List(workspace),
        2 => ImprovementRequest::Suggest {
            workspace,
            run: read_id(r, RunId::new)?,
            proposal: read_text(r)?,
        },
        3 => ImprovementRequest::Dismiss { workspace, candidate: read_digest(r)? },
        4 => ImprovementRequest::Evaluate {
            workspace,
            candidate: read_digest(r)?,
            run: read_run_request(r)?,
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
        if let Some(run) = item.evaluation() {
            write_id(w, run.as_bytes())?;
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
    let count = read_count(r, MAX_IMPROVEMENTS)?;
    let mut candidates = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_digest(r)?;
        let proposal = read_text(r)?;
        let dismissed = r.read_bool()?;
        let evaluation = if r.read_option_tag()? { Some(read_id(r, RunId::new)?) } else { None };
        let count = read_count(r, MAX_IMPROVEMENT_EVIDENCE)?;
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

fn read_count(r: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, CodecError> {
    let value = usize::from(r.read_u16()?);
    if value > maximum {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, r.offset()));
    }
    Ok(value)
}

fn read_text(r: &mut CanonicalReader<'_>) -> Result<ImprovementText, CodecError> {
    let text = r.read_str()?;
    if text.len() > crate::MAX_IMPROVEMENT_TEXT {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, r.offset()));
    }
    invalid(r.offset(), ImprovementText::new(text.to_owned()))
}
