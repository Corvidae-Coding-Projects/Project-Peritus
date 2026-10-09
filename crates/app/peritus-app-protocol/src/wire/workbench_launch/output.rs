//! Preview output codec.

use super::{
    CanonicalReader, CanonicalWriter, CodecError, ControlOperationId, invalid, read_count, read_id,
    read_page, write_count, write_id, write_page,
};
use crate::{WorkbenchPreviewOutput, WorkbenchPreviewSnapshot};

pub(in crate::wire) fn write_preview(
    w: &mut CanonicalWriter,
    value: &WorkbenchPreviewSnapshot,
) -> Result<(), CodecError> {
    write_page(w, value.result())?;
    write_count(w, value.outputs().len())?;
    for output in value.outputs() {
        write_id(w, output.launch().as_bytes())?;
        w.write_str(output.stdout())?;
        w.write_str(output.stderr())?;
        w.write_bool(output.truncated())?;
    }
    Ok(())
}
pub(in crate::wire) fn read_preview(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchPreviewSnapshot, CodecError> {
    let offset = r.offset();
    let result = read_page(r)?;
    let count = read_count(r, usize::from(u16::MAX))?;
    let mut outputs = Vec::with_capacity(count);
    for _ in 0..count {
        let start = r.offset();
        let launch = read_id(r, ControlOperationId::new)?;
        let stdout = r.read_str()?.to_owned();
        let stderr = r.read_str()?.to_owned();
        let truncated = r.read_bool()?;
        outputs
            .push(invalid(start, WorkbenchPreviewOutput::new(launch, stdout, stderr, truncated))?);
    }
    invalid(offset, WorkbenchPreviewSnapshot::new(result, outputs))
}
