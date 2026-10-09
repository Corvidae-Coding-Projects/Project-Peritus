//! Exact paged image metadata; row counts and labels are bounded before owned allocations.

use super::primitive::{invalid, read_id, write_id};
use crate::{
    ControlOperationId, MAX_WORKBENCH_IMAGE_LABEL_BYTES, MAX_WORKBENCH_IMAGE_PAGE,
    WorkbenchImageLabel, WorkbenchImagePage, WorkbenchImageQuery, WorkbenchImageRow,
};
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};
use peritus_types::ArtifactId;

#[cfg(test)]
mod tests;

pub(super) fn write_query(
    w: &mut CanonicalWriter,
    value: WorkbenchImageQuery,
) -> Result<(), CodecError> {
    super::workbench::write_query(w, value.query())?;
    w.write_u64(value.revision())?;
    w.write_u32(value.offset())
}
pub(super) fn read_query(r: &mut CanonicalReader<'_>) -> Result<WorkbenchImageQuery, CodecError> {
    let offset = r.offset();
    let query = super::workbench::read_query(r)?;
    invalid(offset, WorkbenchImageQuery::new(query, r.read_u64()?, r.read_u32()?))
}
pub(super) fn write_page(
    w: &mut CanonicalWriter,
    value: &WorkbenchImagePage,
) -> Result<(), CodecError> {
    write_query(w, value.query())?;
    w.write_u32(value.total())?;
    w.write_u16(
        u16::try_from(value.rows().len())
            .map_err(|_| CodecError::at(CodecErrorKind::LimitExceeded, w.len()))?,
    )?;
    for row in value.rows() {
        write_id(w, row.operation().as_bytes())?;
        write_id(w, row.artifact().as_bytes())?;
        w.write_str(row.label().as_str())?;
        super::workbench_images::write_metadata(w, row.image())?;
        super::workbench_inputs::write_row(w, row.source())?;
        w.write_option_tag(row.selected())?;
        w.write_option_tag(row.eligible())?;
    }
    Ok(())
}
pub(super) fn read_page(r: &mut CanonicalReader<'_>) -> Result<WorkbenchImagePage, CodecError> {
    let offset = r.offset();
    let query = read_query(r)?;
    let total = r.read_u32()?;
    let count = usize::from(r.read_u16()?);
    if count > MAX_WORKBENCH_IMAGE_PAGE {
        return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let operation = read_id(r, ControlOperationId::new)?;
        let artifact = read_id(r, ArtifactId::new)?;
        let label = r.read_str()?;
        if label.len() > MAX_WORKBENCH_IMAGE_LABEL_BYTES {
            return Err(CodecError::at(CodecErrorKind::LimitExceeded, offset));
        }
        let label = invalid(offset, WorkbenchImageLabel::new(label.to_owned()))?;
        let image = super::workbench_images::read_metadata(r)?;
        let source = super::workbench_inputs::read_row(r)?;
        let selected = r.read_option_tag()?;
        let eligible = r.read_option_tag()?;
        rows.push(invalid(
            offset,
            WorkbenchImageRow::new(operation, artifact, label, image, source, (selected, eligible)),
        )?);
    }
    invalid(offset, WorkbenchImagePage::new(query, total, rows))
}
