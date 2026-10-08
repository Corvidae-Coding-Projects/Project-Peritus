//! Exact queue wire forms. All collection bounds are checked before allocation.

use super::primitive::{invalid, read_digest, read_id, unknown, write_digest, write_id};
use crate::{
    MAX_WORKBENCH_INPUT_BYTES, MAX_WORKBENCH_INPUT_PAGE, WorkbenchInputId, WorkbenchInputOrder,
    WorkbenchInputRow, WorkbenchInputSelection, WorkbenchInputSource, WorkbenchInputState,
    WorkbenchInputText,
    WorkbenchNewInput, WorkbenchQueueIntent, WorkbenchQueuePage, WorkbenchQueueQuery,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

pub(super) fn write_selection(
    w: &mut CanonicalWriter,
    value: WorkbenchInputSelection,
) -> Result<(), CodecError> {
    write_id(w, value.id().as_bytes())?;
    w.write_u64(value.revision())
}
pub(super) fn read_selection(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchInputSelection, CodecError> {
    let offset = r.offset();
    let id = read_id(r, WorkbenchInputId::new)?;
    let revision = r.read_u64()?;
    invalid(offset, WorkbenchInputSelection::new(id, revision))
}
pub(super) fn read_text(r: &mut CanonicalReader<'_>) -> Result<WorkbenchInputText, CodecError> {
    let offset = r.offset();
    let text = r.read_str()?;
    if text.len() > MAX_WORKBENCH_INPUT_BYTES {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    invalid(offset, WorkbenchInputText::new(text.to_owned()))
}
fn write_order(w: &mut CanonicalWriter, value: &WorkbenchInputOrder) -> Result<(), CodecError> {
    w.write_u16_collection_len(value.ids().len())?;
    for id in value.ids() {
        write_id(w, id.as_bytes())?;
    }
    Ok(())
}
fn read_order(r: &mut CanonicalReader<'_>) -> Result<WorkbenchInputOrder, CodecError> {
    let offset = r.offset();
    let count = r.read_u16_collection_len(16)?;
    let mut ids = r.reserve_collection(count)?;
    for _ in 0..count {
        ids.push(read_id(r, WorkbenchInputId::new)?);
    }
    invalid(offset, WorkbenchInputOrder::new(ids))
}
fn write_source(w: &mut CanonicalWriter, source: WorkbenchInputSource) -> Result<(), CodecError> {
    write_id(w, source.artifact().as_bytes())?;
    write_digest(w, source.digest())?;
    w.write_u64(source.bytes())
}
fn read_source(r: &mut CanonicalReader<'_>) -> Result<WorkbenchInputSource, CodecError> {
    let offset = r.offset();
    let artifact = read_id(r, peritus_types::ArtifactId::new)?;
    let digest = read_digest(r)?;
    let bytes = r.read_u64()?;
    invalid(offset, WorkbenchInputSource::new(artifact, digest, bytes))
}

pub(super) fn write_intent(
    w: &mut CanonicalWriter,
    value: &WorkbenchQueueIntent,
) -> Result<(), CodecError> {
    match value {
        WorkbenchQueueIntent::Enqueue(input) => {
            w.write_u16(1)?;
            write_id(w, input.id().as_bytes())?;
            w.write_str(input.text().as_str())?;
            write_order(w, input.dependencies())
        }
        WorkbenchQueueIntent::Edit { selected, text } => {
            w.write_u16(2)?;
            write_selection(w, *selected)?;
            w.write_str(text.as_str())
        }
        WorkbenchQueueIntent::Correct { original, id, text } => {
            w.write_u16(3)?;
            write_selection(w, *original)?;
            write_id(w, id.as_bytes())?;
            w.write_str(text.as_str())
        }
        WorkbenchQueueIntent::Hold { selected, held } => {
            w.write_u16(4)?;
            write_selection(w, *selected)?;
            w.write_bool(*held)
        }
        WorkbenchQueueIntent::Withdraw(selected) => {
            w.write_u16(5)?;
            write_selection(w, *selected)
        }
        WorkbenchQueueIntent::Reorder(order) => {
            w.write_u16(6)?;
            write_order(w, order)
        }
        WorkbenchQueueIntent::Move { selected, position } => {
            w.write_u16(7)?;
            write_selection(w, *selected)?;
            w.write_u64(*position)
        }
        WorkbenchQueueIntent::EnqueueSource { id, source, dependencies } => {
            w.write_u16(8)?;
            write_id(w, id.as_bytes())?;
            write_source(w, *source)?;
            write_order(w, dependencies)
        }
        WorkbenchQueueIntent::EditSource { selected, source } => {
            w.write_u16(9)?;
            write_selection(w, *selected)?;
            write_source(w, *source)
        }
        WorkbenchQueueIntent::CorrectSource { original, id, source } => {
            w.write_u16(10)?;
            write_selection(w, *original)?;
            write_id(w, id.as_bytes())?;
            write_source(w, *source)
        }
    }
}
pub(super) fn read_intent(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQueueIntent, CodecError> {
    let offset = r.offset();
    Ok(match r.read_u16()? {
        1 => {
            let id = read_id(r, WorkbenchInputId::new)?;
            let text = read_text(r)?;
            let dependencies = read_order(r)?;
            WorkbenchQueueIntent::Enqueue(invalid(
                offset,
                WorkbenchNewInput::new(id, text, dependencies),
            )?)
        }
        2 => WorkbenchQueueIntent::Edit { selected: read_selection(r)?, text: read_text(r)? },
        3 => WorkbenchQueueIntent::Correct {
            original: read_selection(r)?,
            id: read_id(r, WorkbenchInputId::new)?,
            text: read_text(r)?,
        },
        4 => WorkbenchQueueIntent::Hold { selected: read_selection(r)?, held: r.read_bool()? },
        5 => WorkbenchQueueIntent::Withdraw(read_selection(r)?),
        6 => WorkbenchQueueIntent::Reorder(read_order(r)?),
        7 => WorkbenchQueueIntent::Move {
            selected: read_selection(r)?,
            position: r.read_u64()?,
        },
        8 => WorkbenchQueueIntent::EnqueueSource {
            id: read_id(r, WorkbenchInputId::new)?,
            source: read_source(r)?,
            dependencies: read_order(r)?,
        },
        9 => WorkbenchQueueIntent::EditSource {
            selected: read_selection(r)?,
            source: read_source(r)?,
        },
        10 => WorkbenchQueueIntent::CorrectSource {
            original: read_selection(r)?,
            id: read_id(r, WorkbenchInputId::new)?,
            source: read_source(r)?,
        },
        _ => return unknown(offset),
    })
}

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchQueueQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.offset())?;
    w.write_bool(value.history())
}
pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQueueQuery, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    let revision = r.read_u64()?;
    let start = r.read_u32()?;
    let history = r.read_bool()?;
    invalid(offset, WorkbenchQueueQuery::new(query, revision, start, history))
}
pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchQueuePage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u32(value.total())?;
    w.write_u16_collection_len(value.rows().len())?;
    for row in value.rows() {
        write_row(w, row)?;
    }
    Ok(())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQueuePage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let count = r.read_u16_collection_len(32)?;
    if count > MAX_WORKBENCH_INPUT_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = r.reserve_collection(count)?;
    for _ in 0..count {
        rows.push(read_row(r)?);
    }
    invalid(offset, WorkbenchQueuePage::new(query, total, rows))
}

pub(super) fn write_row(
    w: &mut CanonicalWriter,
    row: &WorkbenchInputRow,
) -> Result<(), CodecError> {
    write_selection(w, row.selected())?;
    w.write_str(row.text().as_str())?;
    let source_offset = if row.source().is_some() { 5 } else { 0 };
    w.write_u16(source_offset + match row.state() {
        WorkbenchInputState::Queued => 1,
        WorkbenchInputState::Held => 2,
        WorkbenchInputState::Incorporated => 3,
        WorkbenchInputState::Superseded => 4,
        WorkbenchInputState::Withdrawn => 5,
    })?;
    if let Some(source) = row.source() {
        write_source(w, source)?;
    }
    write_order(w, row.dependencies())
}

pub(super) fn read_row(r: &mut CanonicalReader<'_>) -> Result<WorkbenchInputRow, CodecError> {
    let selected = read_selection(r)?;
    let text = read_text(r)?;
    let offset = r.offset();
    let kind = r.read_u16()?;
    let state = match kind {
        1 | 6 => WorkbenchInputState::Queued,
        2 | 7 => WorkbenchInputState::Held,
        3 | 8 => WorkbenchInputState::Incorporated,
        4 | 9 => WorkbenchInputState::Superseded,
        5 | 10 => WorkbenchInputState::Withdrawn,
        _ => return unknown(offset),
    };
    let source = (kind > 5).then(|| read_source(r)).transpose()?;
    let dependencies = read_order(r)?;
    match source {
        Some(source) => invalid(
            offset,
            WorkbenchInputRow::new_source(selected, text, source, state, dependencies),
        ),
        None => invalid(offset, WorkbenchInputRow::new(selected, text, state, dependencies)),
    }
}
