//! Exact queue wire forms. All collection bounds are checked before allocation.

use super::primitive::{invalid, read_id, unknown, write_id};
use crate::{
    MAX_WORKBENCH_INPUT_BYTES, MAX_WORKBENCH_INPUT_DEPENDENCIES, MAX_WORKBENCH_INPUT_PAGE,
    MAX_WORKBENCH_PENDING_INPUTS, WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputRow,
    WorkbenchInputSelection, WorkbenchInputState, WorkbenchInputText, WorkbenchNewInput,
    WorkbenchQueueIntent, WorkbenchQueuePage, WorkbenchQueueQuery,
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
    w.write_u16(
        u16::try_from(value.ids().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for id in value.ids() {
        write_id(w, id.as_bytes())?;
    }
    Ok(())
}
fn read_order(
    r: &mut CanonicalReader<'_>,
    bound: usize,
) -> Result<WorkbenchInputOrder, CodecError> {
    let offset = r.offset();
    let count = usize::from(r.read_u16()?);
    if count > bound {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(read_id(r, WorkbenchInputId::new)?);
    }
    invalid(offset, WorkbenchInputOrder::new(ids))
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
    }
}
pub(super) fn read_intent(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQueueIntent, CodecError> {
    let offset = r.offset();
    Ok(match r.read_u16()? {
        1 => {
            let id = read_id(r, WorkbenchInputId::new)?;
            let text = read_text(r)?;
            let dependencies = read_order(r, MAX_WORKBENCH_INPUT_DEPENDENCIES)?;
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
        6 => WorkbenchQueueIntent::Reorder(read_order(r, MAX_WORKBENCH_PENDING_INPUTS)?),
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
    w.write_u16(
        u16::try_from(value.rows().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for row in value.rows() {
        write_row(w, row)?;
    }
    Ok(())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchQueuePage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_INPUT_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = Vec::with_capacity(count);
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
    w.write_u16(match row.state() {
        WorkbenchInputState::Queued => 1,
        WorkbenchInputState::Held => 2,
        WorkbenchInputState::Incorporated => 3,
        WorkbenchInputState::Superseded => 4,
        WorkbenchInputState::Withdrawn => 5,
    })?;
    write_order(w, row.dependencies())
}

pub(super) fn read_row(r: &mut CanonicalReader<'_>) -> Result<WorkbenchInputRow, CodecError> {
    let selected = read_selection(r)?;
    let text = read_text(r)?;
    let offset = r.offset();
    let state = match r.read_u16()? {
        1 => WorkbenchInputState::Queued,
        2 => WorkbenchInputState::Held,
        3 => WorkbenchInputState::Incorporated,
        4 => WorkbenchInputState::Superseded,
        5 => WorkbenchInputState::Withdrawn,
        _ => return unknown(offset),
    };
    let dependencies = read_order(r, MAX_WORKBENCH_INPUT_DEPENDENCIES)?;
    invalid(offset, WorkbenchInputRow::new(selected, text, state, dependencies))
}
