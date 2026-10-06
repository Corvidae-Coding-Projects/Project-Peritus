//! Closed versioned counts, operation tags, and object identity encoding.

use super::{
    CanonicalReader, CanonicalWriter, CodecLimits, DirectoryMode, FileMode, PatchOperationKind,
    Sha256Digest, TargetIdentity,
};

pub(super) fn write_count(
    writer: &mut CanonicalWriter,
    count: usize,
    schema: u16,
) -> Result<(), peritus_codec::CodecError> {
    if schema == 1 { writer.write_collection_len(count) } else { writer.write_u64(count as u64) }
}

pub(super) fn read_count(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
    minimum_bytes: usize,
) -> Option<usize> {
    let count = if schema == 1 {
        reader.read_collection_len(minimum_bytes).ok()?
    } else {
        usize::try_from(reader.read_u64().ok()?).ok()?
    };
    (count <= reader.remaining() / minimum_bytes
        && (schema != 1 || count <= CodecLimits::LEGACY_V1.max_collection_items))
        .then_some(count)
}

pub(super) fn write_identity(
    writer: &mut CanonicalWriter,
    identity: Option<TargetIdentity>,
) -> Result<(), peritus_codec::CodecError> {
    match identity {
        None => writer.write_u8(0)?,
        Some(TargetIdentity::File { digest, size, mode }) => {
            writer.write_u8(1)?;
            writer.write_fixed(digest.as_bytes())?;
            writer.write_u64(size)?;
            writer.write_u8(mode.tag())?;
        }
        Some(TargetIdentity::EmptyDirectory { mode }) => {
            writer.write_u8(2)?;
            writer.write_u16(mode.bits())?;
        }
    }
    Ok(())
}

pub(super) fn read_identity(
    reader: &mut CanonicalReader<'_>,
    schema: u16,
) -> Result<Option<TargetIdentity>, ()> {
    match reader.read_u8().map_err(|_| ())? {
        0 => return Ok(None),
        1 => {}
        2 if schema >= 3 => {
            return Ok(Some(TargetIdentity::EmptyDirectory {
                mode: DirectoryMode::new(reader.read_u16().map_err(|_| ())?).map_err(|_| ())?,
            }));
        }
        _ => return Err(()),
    }
    let digest = Sha256Digest::new(reader.read_fixed::<32>().map_err(|_| ())?);
    let size = reader.read_u64().map_err(|_| ())?;
    if schema == 1 && size > crate::set::LEGACY_FILE_BYTES as u64 {
        return Err(());
    }
    let mode = FileMode::from_tag(reader.read_u8().map_err(|_| ())?).ok_or(())?;
    Ok(Some(TargetIdentity::File { digest, size, mode }))
}

pub(super) const fn kind_tag(kind: PatchOperationKind) -> u8 {
    match kind {
        PatchOperationKind::Create => 1,
        PatchOperationKind::Replace => 2,
        PatchOperationKind::Delete => 3,
        PatchOperationKind::CreateDirectory => 4,
        PatchOperationKind::DeleteDirectory => 5,
    }
}

pub(super) const fn kind_from_tag(tag: u8) -> Option<PatchOperationKind> {
    match tag {
        1 => Some(PatchOperationKind::Create),
        2 => Some(PatchOperationKind::Replace),
        3 => Some(PatchOperationKind::Delete),
        4 => Some(PatchOperationKind::CreateDirectory),
        5 => Some(PatchOperationKind::DeleteDirectory),
        _ => None,
    }
}

pub(super) const fn shape_valid(
    kind: PatchOperationKind,
    preimage: Option<TargetIdentity>,
    postimage: Option<TargetIdentity>,
) -> bool {
    matches!(
        (kind, preimage, postimage),
        (PatchOperationKind::Create, None, Some(TargetIdentity::File { .. }))
            | (
                PatchOperationKind::Replace,
                Some(TargetIdentity::File { .. } | TargetIdentity::EmptyDirectory { .. }),
                Some(TargetIdentity::File { .. } | TargetIdentity::EmptyDirectory { .. })
            )
            | (PatchOperationKind::Delete, Some(TargetIdentity::File { .. }), None)
            | (
                PatchOperationKind::CreateDirectory,
                None,
                Some(TargetIdentity::EmptyDirectory { .. })
            )
            | (
                PatchOperationKind::DeleteDirectory,
                Some(TargetIdentity::EmptyDirectory { .. }),
                None
            )
    )
}
