//! Canonical conversation-library and fork codecs.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{
    ControlOperationId, ConversationId, ConversationLibraryItem, ConversationLibraryPage,
    ConversationLibraryQuery, ConversationMessageSource, ConversationSearchSnippet,
    ConversationSearchText, WorkbenchBranchLineage, WorkbenchForkMode, WorkbenchForkRequest,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::WorkspaceId;

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: &ConversationLibraryQuery,
) -> Result<(), CodecError> {
    write_id(w, value.workspace().as_bytes())?;
    w.write_bool(value.literal().is_some())?;
    if let Some(literal) = value.literal() {
        w.write_str(literal.as_str())?;
    }
    w.write_bool(value.include_archived())?;
    w.write_u32(value.offset())?;
    w.write_u16(value.limit())
}

pub(super) fn read_query(
    r: &mut CanonicalReader<'_>,
) -> Result<ConversationLibraryQuery, CodecError> {
    let offset = r.offset();
    let workspace = read_id(r, WorkspaceId::new)?;
    let literal = if r.read_bool()? {
        let text_offset = r.offset();
        let text = r.read_str()?;
        if text.len() > 256 {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, text_offset));
        }
        Some(invalid(text_offset, ConversationSearchText::new(text.to_owned()))?)
    } else {
        None
    };
    invalid(
        offset,
        ConversationLibraryQuery::new(
            workspace,
            literal,
            r.read_bool()?,
            r.read_u32()?,
            r.read_u16()?,
        ),
    )
}

pub(super) fn write_fork(
    w: &mut CanonicalWriter,
    value: &WorkbenchForkRequest,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.child())?;
    w.write_str(value.title().as_str())?;
    write_id(w, value.checkpoint().as_bytes())?;
    w.write_u64(value.source_revision())?;
    w.write_u64(value.context_generation())?;
    w.write_u64(value.brief_revision())?;
    w.write_u64(value.goal_revision())?;
    write_mode(w, value.mode())?;
    Ok(())
}

pub(super) fn read_fork(r: &mut CanonicalReader<'_>) -> Result<WorkbenchForkRequest, CodecError> {
    let offset = r.offset();
    let child = super::workbench::read_query(r)?;
    let title = super::workbench::read_title(r)?;
    let checkpoint = read_id(r, ControlOperationId::new)?;
    let source_revision = r.read_u64()?;
    let context_generation = r.read_u64()?;
    let brief_revision = r.read_u64()?;
    let goal_revision = r.read_u64()?;
    let mode = read_mode(r)?;
    invalid(
        offset,
        WorkbenchForkRequest::new(
            child,
            title,
            checkpoint,
            source_revision,
            context_generation,
            brief_revision,
            goal_revision,
            mode,
        ),
    )
}

pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &ConversationLibraryPage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u32(value.total())?;
    w.write_bool(value.next_offset().is_some())?;
    if let Some(next) = value.next_offset() {
        w.write_u32(next)?;
    }
    w.write_u16(
        u16::try_from(value.items().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, w.len()))?,
    )?;
    for item in value.items() {
        write_item(w, item)?;
    }
    Ok(())
}

pub(super) fn read_page(
    r: &mut CanonicalReader<'_>,
) -> Result<ConversationLibraryPage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let next = if r.read_bool()? { Some(r.read_u32()?) } else { None };
    let len = usize::from(r.read_u16()?);
    if len > usize::from(query.limit()) {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut items = Vec::with_capacity(len);
    for _ in 0..len {
        items.push(read_item(r)?);
    }
    invalid(offset, ConversationLibraryPage::new(query, total, next, items))
}

fn write_item(w: &mut CanonicalWriter, value: &ConversationLibraryItem) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_str(value.title().as_str())?;
    w.write_bool(value.pinned())?;
    w.write_bool(value.archived())?;
    w.write_u64(value.activity_revision())?;
    w.write_bool(value.goal_state().is_some())?;
    if let Some(state) = value.goal_state() {
        super::workbench_goal::write_state(w, state)?;
    }
    w.write_bool(value.goal_draft())?;
    w.write_str(value.handoff())?;
    w.write_bool(value.snippet().is_some())?;
    if let Some(snippet) = value.snippet() {
        write_snippet(w, snippet)?;
    }
    w.write_bool(value.branch().is_some())?;
    if let Some(branch) = value.branch() {
        write_lineage(w, branch)?;
    }
    Ok(())
}

fn read_item(r: &mut CanonicalReader<'_>) -> Result<ConversationLibraryItem, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let title = super::workbench::read_title(r)?;
    let pinned = r.read_bool()?;
    let archived = r.read_bool()?;
    let activity_revision = r.read_u64()?;
    let goal_state =
        if r.read_bool()? { Some(super::workbench_goal::read_state(r)?) } else { None };
    let goal_draft = r.read_bool()?;
    let handoff_offset = r.offset();
    let handoff = r.read_str()?.to_owned();
    if handoff.len() > 1024 {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, handoff_offset));
    }
    let snippet = if r.read_bool()? { Some(read_snippet(r)?) } else { None };
    let branch = if r.read_bool()? { Some(read_lineage(r)?) } else { None };
    invalid(
        offset,
        ConversationLibraryItem::new(
            query,
            title,
            pinned,
            archived,
            activity_revision,
            goal_state,
            goal_draft,
            handoff,
            snippet,
            branch,
        ),
    )
}

fn write_snippet(
    w: &mut CanonicalWriter,
    value: &ConversationSearchSnippet,
) -> Result<(), CodecError> {
    match value.source() {
        ConversationMessageSource::Input { conversation, input, revision } => {
            w.write_u16(1)?;
            write_id(w, conversation.as_bytes())?;
            write_id(w, input.as_bytes())?;
            w.write_u64(revision)?;
        }
        ConversationMessageSource::Reply { conversation, operation } => {
            w.write_u16(2)?;
            write_id(w, conversation.as_bytes())?;
            write_id(w, operation.as_bytes())?;
        }
    }
    w.write_str(value.text())
}

fn read_snippet(r: &mut CanonicalReader<'_>) -> Result<ConversationSearchSnippet, CodecError> {
    let offset = r.offset();
    let source = match r.read_u16()? {
        1 => ConversationMessageSource::Input {
            conversation: read_id(r, ConversationId::new)?,
            input: read_id(r, crate::WorkbenchInputId::new)?,
            revision: r.read_u64()?,
        },
        2 => ConversationMessageSource::Reply {
            conversation: read_id(r, ConversationId::new)?,
            operation: read_id(r, ControlOperationId::new)?,
        },
        _ => return unknown(offset),
    };
    let text_offset = r.offset();
    let text = r.read_str()?.to_owned();
    if text.len() > 512 {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, text_offset));
    }
    invalid(offset, ConversationSearchSnippet::new(source, text))
}

fn write_lineage(
    w: &mut CanonicalWriter,
    value: &WorkbenchBranchLineage,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.parent())?;
    write_id(w, value.checkpoint().as_bytes())?;
    w.write_u64(value.source_revision())?;
    w.write_u64(value.context_generation())?;
    w.write_u64(value.brief_revision())?;
    w.write_u64(value.goal_revision())?;
    write_mode(w, value.mode())?;
    Ok(())
}

fn read_lineage(r: &mut CanonicalReader<'_>) -> Result<WorkbenchBranchLineage, CodecError> {
    let parent = super::workbench::read_query(r)?;
    let checkpoint = read_id(r, ControlOperationId::new)?;
    let source_revision = r.read_u64()?;
    let context_generation = r.read_u64()?;
    let brief_revision = r.read_u64()?;
    let goal_revision = r.read_u64()?;
    let mode = read_mode(r)?;
    Ok(WorkbenchBranchLineage::new(
        parent,
        checkpoint,
        source_revision,
        context_generation,
        brief_revision,
        goal_revision,
        mode,
    ))
}

fn write_mode(w: &mut CanonicalWriter, value: WorkbenchForkMode) -> Result<(), CodecError> {
    w.write_u16(match value {
        WorkbenchForkMode::ReadOnlyCurrentWorkspace => 1,
        WorkbenchForkMode::IsolatedWritableWorkspace => 2,
    })
}
fn read_mode(r: &mut CanonicalReader<'_>) -> Result<WorkbenchForkMode, CodecError> {
    let offset = r.offset();
    match r.read_u16()? {
        1 => Ok(WorkbenchForkMode::ReadOnlyCurrentWorkspace),
        2 => Ok(WorkbenchForkMode::IsolatedWritableWorkspace),
        _ => unknown(offset),
    }
}
