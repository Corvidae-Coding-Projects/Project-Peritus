//! Schema-four physical pages under one checksum and one atomic transaction publication.

use std::io::Write;

use sha2::{Digest as _, Sha256};

use super::{
    CanonicalReader, CanonicalWriter, CodecLimits, MAGIC, Manifest, ManifestEntry, PatchError,
    PatchOperationContext, RollbackStatus, SNAPSHOT_METADATA_LIMITS, WorkspacePath,
    corrupt_manifest, kind_from_tag, kind_tag, read_identity, shape_valid, write_identity,
};

// A storage chunk, not a mutation allowance: there is no maximum number of pages.
const PAGE_BYTES: usize = 64 * 1024;
const PAGE_LIMITS: CodecLimits = CodecLimits {
    max_frame_bytes: PAGE_BYTES,
    max_payload_bytes: PAGE_BYTES,
    max_opaque_bytes: PAGE_BYTES,
    ..SNAPSHOT_METADATA_LIMITS
};

pub(super) fn write(manifest: &Manifest, output: &mut dyn Write) -> Result<(), PatchError> {
    let mut header = CanonicalWriter::new(PAGE_LIMITS);
    let encoded = (|| {
        header.write_fixed(MAGIC)?;
        header.write_u16(4)?;
        header.write_u8(manifest.phase.tag())?;
        header.write_fixed(manifest.workspace_id.as_bytes())?;
        header.write_u64(manifest.generation.get())?;
        header.write_u64(manifest.revision.get())?;
        header.write_fixed(manifest.identity.as_bytes())?;
        header.write_u64(manifest.entries.len() as u64)
    })();
    encoded.map_err(|_| corrupt_manifest())?;
    let mut hasher = Sha256::new();
    emit(output, &mut hasher, header.as_slice())?;
    write_pages(output, &mut hasher, &manifest.entries, encode_entry)?;
    emit(output, &mut hasher, &(manifest.created_directories.len() as u64).to_be_bytes())?;
    write_pages(output, &mut hasher, &manifest.created_directories, encode_directory)?;
    output.write_all(&hasher.finalize()).map_err(storage_error)
}

fn write_pages<T>(
    output: &mut dyn Write,
    hasher: &mut Sha256,
    items: &[T],
    encode: fn(&T) -> Result<Vec<u8>, PatchError>,
) -> Result<(), PatchError> {
    let mut page = Vec::with_capacity(PAGE_BYTES);
    let mut count = 0_u64;
    for item in items {
        let record = encode(item)?;
        if page.len() + record.len() > PAGE_BYTES {
            emit_page(output, hasher, &page, count)?;
            page.clear();
            count = 0;
        }
        page.extend_from_slice(&record);
        count += 1;
    }
    if count != 0 {
        emit_page(output, hasher, &page, count)?;
    }
    Ok(())
}

fn emit_page(
    output: &mut dyn Write,
    hasher: &mut Sha256,
    page: &[u8],
    count: u64,
) -> Result<(), PatchError> {
    let length = u32::try_from(page.len()).map_err(|_| corrupt_manifest())?;
    emit(output, hasher, &count.to_be_bytes())?;
    emit(output, hasher, &length.to_be_bytes())?;
    emit(output, hasher, page)?;
    emit(output, hasher, peritus_codec::sha256(page).as_bytes())
}

fn emit(output: &mut dyn Write, hasher: &mut Sha256, bytes: &[u8]) -> Result<(), PatchError> {
    output.write_all(bytes).map_err(storage_error)?;
    hasher.update(bytes);
    Ok(())
}

fn encode_entry(entry: &ManifestEntry) -> Result<Vec<u8>, PatchError> {
    let mut writer = CanonicalWriter::new(PAGE_LIMITS);
    let result = (|| {
        writer.write_u8(kind_tag(entry.kind))?;
        writer.write_str(entry.path.as_str())?;
        write_identity(&mut writer, entry.preimage)?;
        write_identity(&mut writer, entry.postimage)
    })();
    result.map_err(|_| corrupt_manifest())?;
    Ok(writer.into_bytes())
}

fn encode_directory(directory: &WorkspacePath) -> Result<Vec<u8>, PatchError> {
    let mut writer = CanonicalWriter::new(PAGE_LIMITS);
    writer.write_str(directory.as_str()).map_err(|_| corrupt_manifest())?;
    Ok(writer.into_bytes())
}

pub(super) fn read_entries(
    reader: &mut CanonicalReader<'_>,
    count: usize,
) -> Option<Vec<ManifestEntry>> {
    read_pages(reader, count, 8, |page| {
        let kind = kind_from_tag(page.read_u8().ok()?)?;
        let path = WorkspacePath::new(page.read_str().ok()?).ok()?;
        if !path.is_legacy_portable() {
            return None;
        }
        let preimage = read_identity(page, 4).ok()?;
        let postimage = read_identity(page, 4).ok()?;
        shape_valid(kind, preimage, postimage).then_some(ManifestEntry {
            kind,
            path,
            preimage,
            postimage,
        })
    })
}

pub(super) fn read_directories(
    reader: &mut CanonicalReader<'_>,
    count: usize,
) -> Option<Vec<WorkspacePath>> {
    read_pages(reader, count, 5, |page| {
        let path = WorkspacePath::new(page.read_str().ok()?).ok()?;
        path.is_legacy_portable().then_some(path)
    })
}

fn read_pages<T>(
    reader: &mut CanonicalReader<'_>,
    count: usize,
    minimum_record_bytes: usize,
    decode: fn(&mut CanonicalReader<'_>) -> Option<T>,
) -> Option<Vec<T>> {
    let mut items = Vec::new();
    let mut previous_page_bytes = None;
    while items.len() < count {
        let page_count = usize::try_from(reader.read_u64().ok()?).ok()?;
        let bytes = reader.read_bytes().ok()?;
        let checksum = reader.read_fixed::<32>().ok()?;
        if bytes.len() > PAGE_BYTES
            || page_count == 0
            || page_count > count - items.len()
            || page_count > bytes.len() / minimum_record_bytes
            || peritus_codec::sha256(bytes).as_bytes() != &checksum
        {
            return None;
        }
        let mut page = CanonicalReader::new(bytes, PAGE_LIMITS);
        for index in 0..page_count {
            let before = page.remaining();
            let item = decode(&mut page)?;
            // Reject alternative partitions of the same metadata. A non-final page must
            // be full enough that the next record could not have fit in it.
            if index == 0
                && previous_page_bytes
                    .is_some_and(|previous| previous + before - page.remaining() <= PAGE_BYTES)
            {
                return None;
            }
            items.push(item);
        }
        page.finish().ok()?;
        previous_page_bytes = Some(bytes.len());
    }
    Some(items)
}

fn storage_error(error: std::io::Error) -> PatchError {
    PatchError::io(PatchOperationContext::PersistManifest, RollbackStatus::NotRequired, error)
}
