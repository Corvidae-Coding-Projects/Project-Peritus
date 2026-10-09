//! Bounded canonical file pages; row count is checked before allocation.
use super::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, invalid, read_id, read_metadata,
    read_mode, read_string, write_id, write_metadata, write_mode,
};
use crate::{ControlOperationId, WorkbenchFilePage, WorkbenchFileQuery, WorkbenchFileRow};

pub(in crate::wire) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchFileQuery,
) -> Result<(), CodecError> {
    crate::wire::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.offset())
}
pub(in crate::wire) fn read_query(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFileQuery, CodecError> {
    let offset = r.offset();
    let query = crate::wire::workbench::read_query(r)?;
    invalid(offset, WorkbenchFileQuery::new(query, r.read_u64()?, r.read_u32()?))
}
pub(in crate::wire) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchFilePage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u32(value.total())?;
    w.write_u32(
        u32::try_from(value.rows().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, 0))?,
    )?;
    for row in value.rows() {
        write_id(w, row.attachment().as_bytes())?;
        write_id(w, row.version().as_bytes())?;
        w.write_str(row.label())?;
        write_mode(w, row.mode())?;
        write_metadata(w, row.file())?;
        w.write_bool(row.selected())?;
        w.write_bool(row.eligible())?;
    }
    Ok(())
}
pub(in crate::wire) fn read_page(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchFilePage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let count = r.read_u32()?;
    if count > 32 || total > 256 {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let attachment = read_id(r, ControlOperationId::new)?;
        let version = read_id(r, ControlOperationId::new)?;
        let label = read_string(r, 4096)?;
        let mode = read_mode(r)?;
        let file = read_metadata(r)?;
        rows.push(invalid(
            offset,
            WorkbenchFileRow::new(
                attachment,
                version,
                label,
                mode,
                file,
                r.read_bool()?,
                r.read_bool()?,
            ),
        )?);
    }
    invalid(offset, WorkbenchFilePage::new(query, rows, total))
}
