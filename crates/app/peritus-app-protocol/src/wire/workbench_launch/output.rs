//! Preview output codec.

use super::{
    CanonicalReader, CanonicalWriter, CodecError, ControlOperationId, invalid, read_count, read_id,
    read_page, write_count, write_id, write_page,
};
use crate::{
    WorkbenchPreviewOutput, WorkbenchPreviewOutputRange, WorkbenchPreviewOutputStream,
    WorkbenchPreviewSnapshot,
};

pub(in crate::wire) fn write_output_range(
    w: &mut CanonicalWriter,
    value: &WorkbenchPreviewOutputRange,
) -> Result<(), CodecError> {
    write_id(w, value.launch().as_bytes())?;
    w.write_u16(stream_tag(value.stream()))?;
    w.write_u64(value.offset())?;
    w.write_u64(value.total_bytes())?;
    match value.artifact_digest() {
        Some(digest) => {
            w.write_bool(true)?;
            w.write_fixed(&digest)?;
        }
        None => w.write_bool(false)?,
    }
    w.write_bytes(value.bytes())
}

pub(in crate::wire) fn read_output_range(
    r: &mut CanonicalReader<'_>,
) -> Result<WorkbenchPreviewOutputRange, CodecError> {
    let start = r.offset();
    let launch = read_id(r, ControlOperationId::new)?;
    let stream_offset = r.offset();
    let stream = read_stream(r.read_u16()?, stream_offset)?;
    let offset = r.read_u64()?;
    let total_bytes = r.read_u64()?;
    let artifact_digest = if r.read_bool()? { Some(r.read_fixed()?) } else { None };
    let bytes = r.read_bytes_owned()?;
    if bytes.len() > crate::MAX_WORKBENCH_PREVIEW_OUTPUT_RANGE_BYTES {
        return Err(CodecError::at(peritus_codec::CodecErrorKind::LimitExceeded, start));
    }
    invalid(
        start,
        WorkbenchPreviewOutputRange::new(
            launch,
            stream,
            offset,
            total_bytes,
            artifact_digest,
            bytes,
        ),
    )
}

const fn stream_tag(stream: WorkbenchPreviewOutputStream) -> u16 {
    match stream {
        WorkbenchPreviewOutputStream::Stdout => 1,
        WorkbenchPreviewOutputStream::Stderr => 2,
        WorkbenchPreviewOutputStream::Terminal => 3,
    }
}

const fn read_stream(tag: u16, offset: usize) -> Result<WorkbenchPreviewOutputStream, CodecError> {
    match tag {
        1 => Ok(WorkbenchPreviewOutputStream::Stdout),
        2 => Ok(WorkbenchPreviewOutputStream::Stderr),
        3 => Ok(WorkbenchPreviewOutputStream::Terminal),
        _ => Err(CodecError::at(peritus_codec::CodecErrorKind::UnknownTag, offset)),
    }
}

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
