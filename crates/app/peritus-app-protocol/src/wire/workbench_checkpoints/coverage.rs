//! Additive typed coverage tags; legacy whole-file encodings stay byte-for-byte identical.

use super::{
    CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind, invalid, read_digest, unknown,
    write_digest,
};
use crate::{
    WorkbenchCheckpointFileMode as FileMode, WorkbenchCheckpointRange,
    WorkbenchCheckpointVersion as Version, WorkbenchFileRange,
};

pub(super) fn write_version(w: &mut CanonicalWriter, value: Version) -> Result<(), CodecError> {
    match value {
        Version::Absent => w.write_u16(0),
        Version::EmptyDirectory { permissions } => {
            w.write_u16(2)?;
            w.write_u16(permissions)
        }
        Version::Present { digest, bytes, mode } => {
            w.write_u16(1)?;
            write_digest(w, digest)?;
            w.write_u64(bytes)?;
            w.write_u16(match mode {
                FileMode::Regular => 1,
                FileMode::Executable => 2,
            })
        }
    }
}

pub(super) fn read_version(r: &mut CanonicalReader<'_>) -> Result<Version, CodecError> {
    let tag = r.read_u16()?;
    read_version_tag(r, tag)
}

fn read_version_tag(r: &mut CanonicalReader<'_>, tag: u16) -> Result<Version, CodecError> {
    let offset = r.offset();
    match tag {
        0 => Ok(Version::Absent),
        2 => {
            let permissions = r.read_u16()?;
            if permissions > 0o7777 {
                return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
            }
            Ok(Version::EmptyDirectory { permissions })
        }
        1 => {
            let digest = read_digest(r)?;
            let bytes = r.read_u64()?;
            let mode = match r.read_u16()? {
                1 => FileMode::Regular,
                2 => FileMode::Executable,
                _ => return unknown(offset),
            };
            Ok(Version::Present { digest, bytes, mode })
        }
        _ => unknown(offset),
    }
}

pub(super) fn write_captured(
    w: &mut CanonicalWriter,
    version: Version,
    ranges: &[WorkbenchCheckpointRange],
) -> Result<(), CodecError> {
    if ranges.is_empty() {
        return write_version(w, version);
    }
    // A new outer tag makes the selected scope explicit. It cannot be decoded as an old
    // whole-file promise, and directory/file identity remains separately typed inside it.
    w.write_u16(3)?;
    write_version(w, version)?;
    w.write_u64(ranges.len() as u64)?;
    for range in ranges {
        match range.selection() {
            WorkbenchFileRange::Bytes { start, end } => {
                w.write_u16(1)?;
                w.write_u64(start)?;
                w.write_u64(end)?;
            }
            WorkbenchFileRange::Lines { first, last } => {
                w.write_u16(2)?;
                w.write_u32(first)?;
                w.write_u32(last)?;
            }
            WorkbenchFileRange::All => {
                return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, w.len()));
            }
        }
        let (start, end) = range.captured_interval();
        w.write_u64(start)?;
        w.write_u64(end)?;
    }
    Ok(())
}

pub(super) fn read_captured(
    r: &mut CanonicalReader<'_>,
) -> Result<(Version, Vec<WorkbenchCheckpointRange>), CodecError> {
    let offset = r.offset();
    let tag = r.read_u16()?;
    if tag != 3 {
        return Ok((read_version_tag(r, tag)?, Vec::new()));
    }
    let version = read_version(r)?;
    let count = usize::try_from(r.read_u64()?)
        .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
    if count == 0 || count > r.remaining() / 26 {
        return Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset));
    }
    let mut ranges = Vec::with_capacity(count);
    for _ in 0..count {
        let selection = match r.read_u16()? {
            1 => WorkbenchFileRange::Bytes { start: r.read_u64()?, end: r.read_u64()? },
            2 => WorkbenchFileRange::Lines { first: r.read_u32()?, last: r.read_u32()? },
            _ => return unknown(offset),
        };
        ranges.push(invalid(
            offset,
            WorkbenchCheckpointRange::new(selection, r.read_u64()?, r.read_u64()?),
        )?);
    }
    Ok((version, ranges))
}
