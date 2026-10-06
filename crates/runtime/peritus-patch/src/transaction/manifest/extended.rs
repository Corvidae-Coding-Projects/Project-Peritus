//! Native path metadata split across physical byte pages without a record-size allowance.

use super::{
    CanonicalReader, CanonicalWriter, MAGIC, Manifest, ManifestEntry, PatchError,
    PatchOperationContext, RollbackStatus, SNAPSHOT_METADATA_LIMITS, WorkspacePath,
    corrupt_manifest, kind_from_tag, kind_tag, read_identity, shape_valid, write_identity,
};
use sha2::{Digest as _, Sha256};
use std::io::{self, Read as _, Write as _};

const PAGE_BYTES: usize = 64 * 1024;

pub(super) fn write(manifest: &Manifest, output: &mut dyn io::Write) -> Result<(), PatchError> {
    let mut header = CanonicalWriter::new(SNAPSHOT_METADATA_LIMITS);
    let result = (|| {
        header.write_fixed(MAGIC)?;
        header.write_u16(5)?;
        header.write_u8(crate::path::native_platform_tag())?;
        header.write_u8(manifest.phase.tag())?;
        header.write_fixed(manifest.workspace_id.as_bytes())?;
        header.write_u64(manifest.generation.get())?;
        header.write_u64(manifest.revision.get())?;
        header.write_fixed(manifest.identity.as_bytes())?;
        header.write_u64(manifest.entries.len() as u64)
    })();
    result.map_err(|_| corrupt_manifest())?;
    let mut hasher = Sha256::new();
    emit(output, &mut hasher, header.as_slice()).map_err(storage_error)?;
    let mut pages =
        PageWriter { output, hasher: &mut hasher, bytes: Vec::with_capacity(PAGE_BYTES) };
    for entry in &manifest.entries {
        pages.write_all(&[kind_tag(entry.kind)]).map_err(storage_error)?;
        write_path(&mut pages, &entry.path)?;
        let mut identity = CanonicalWriter::new(SNAPSHOT_METADATA_LIMITS);
        write_identity(&mut identity, entry.preimage).map_err(|_| corrupt_manifest())?;
        write_identity(&mut identity, entry.postimage).map_err(|_| corrupt_manifest())?;
        pages.write_all(identity.as_slice()).map_err(storage_error)?;
    }
    pages
        .write_all(&(manifest.created_directories.len() as u64).to_be_bytes())
        .map_err(storage_error)?;
    for directory in &manifest.created_directories {
        write_path(&mut pages, directory)?;
    }
    pages.finish().map_err(storage_error)?;
    output.write_all(&hasher.finalize()).map_err(storage_error)
}

fn write_path(output: &mut impl io::Write, path: &WorkspacePath) -> Result<(), PatchError> {
    output.write_all(&(path.as_str().len() as u64).to_be_bytes()).map_err(storage_error)?;
    output.write_all(path.as_str().as_bytes()).map_err(storage_error)
}

struct PageWriter<'a> {
    output: &'a mut dyn io::Write,
    hasher: &'a mut Sha256,
    bytes: Vec<u8>,
}
impl PageWriter<'_> {
    fn page(&mut self) -> io::Result<()> {
        let length = u32::try_from(self.bytes.len()).map_err(|_| corrupt_page())?;
        emit(self.output, self.hasher, &length.to_be_bytes())?;
        emit(self.output, self.hasher, &self.bytes)?;
        emit(self.output, self.hasher, peritus_codec::sha256(&self.bytes).as_bytes())?;
        self.bytes.clear();
        Ok(())
    }
    fn finish(mut self) -> io::Result<()> {
        if !self.bytes.is_empty() {
            self.page()?;
        }
        emit(self.output, self.hasher, &0_u32.to_be_bytes())
    }
}
impl io::Write for PageWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let count = bytes.len().min(PAGE_BYTES - self.bytes.len());
        self.bytes.extend_from_slice(&bytes[..count]);
        if self.bytes.len() == PAGE_BYTES {
            self.page()?;
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

fn emit(output: &mut dyn io::Write, hasher: &mut Sha256, bytes: &[u8]) -> io::Result<()> {
    output.write_all(bytes)?;
    hasher.update(bytes);
    Ok(())
}

pub(super) fn read(
    reader: &mut CanonicalReader<'_>,
    count: usize,
) -> Option<(Vec<ManifestEntry>, Vec<WorkspacePath>)> {
    let mut pages = PageReader {
        remaining_bound: reader.remaining(),
        reader,
        bytes: &[],
        offset: 0,
        last_size: None,
        ended: false,
    };
    let mut entries = Vec::new();
    for _ in 0..count {
        let kind = kind_from_tag(pages.fixed::<1>()?[0])?;
        let path = pages.path()?;
        let preimage = pages.identity().ok()?;
        let postimage = pages.identity().ok()?;
        if !shape_valid(kind, preimage, postimage) {
            return None;
        }
        entries.push(ManifestEntry { kind, path, preimage, postimage });
    }
    let count = usize::try_from(u64::from_be_bytes(pages.fixed::<8>()?)).ok()?;
    if count > pages.remaining_bound / 9 {
        return None;
    }
    let mut directories = Vec::new();
    for _ in 0..count {
        directories.push(pages.path()?);
    }
    pages.finish()?;
    Some((entries, directories))
}

struct PageReader<'a, 'b> {
    reader: &'a mut CanonicalReader<'b>,
    bytes: &'b [u8],
    offset: usize,
    last_size: Option<usize>,
    remaining_bound: usize,
    ended: bool,
}
impl PageReader<'_, '_> {
    fn load(&mut self) -> Option<()> {
        let bytes = self.reader.read_bytes().ok()?;
        if bytes.is_empty() {
            self.ended = true;
            return Some(());
        }
        if bytes.len() > PAGE_BYTES || self.last_size.is_some_and(|size| size != PAGE_BYTES) {
            return None;
        }
        if self.reader.read_fixed::<32>().ok()? != *peritus_codec::sha256(bytes).as_bytes() {
            return None;
        }
        self.last_size = Some(bytes.len());
        self.bytes = bytes;
        self.offset = 0;
        Some(())
    }
    fn fixed<const N: usize>(&mut self) -> Option<[u8; N]> {
        let mut bytes = [0; N];
        self.read_exact(&mut bytes).ok()?;
        Some(bytes)
    }
    fn path(&mut self) -> Option<WorkspacePath> {
        let length = usize::try_from(u64::from_be_bytes(self.fixed::<8>()?)).ok()?;
        if length == 0 || length > self.remaining_bound {
            return None;
        }
        let mut bytes = vec![0; length];
        self.read_exact(&mut bytes).ok()?;
        WorkspacePath::new(String::from_utf8(bytes).ok()?).ok()
    }
    fn identity(&mut self) -> Result<Option<super::TargetIdentity>, ()> {
        let tag = self.fixed::<1>().ok_or(())?[0];
        let mut bytes = vec![tag];
        let size = match tag {
            0 => 0,
            1 => 41,
            2 => 2,
            _ => return Err(()),
        };
        let mut body = vec![0; size];
        self.read_exact(&mut body).map_err(|_| ())?;
        bytes.extend(body);
        read_identity(&mut CanonicalReader::new(&bytes, SNAPSHOT_METADATA_LIMITS), 5)
    }
    fn finish(mut self) -> Option<()> {
        if self.offset != self.bytes.len() || self.ended {
            return None;
        }
        self.load()?;
        self.ended.then_some(())
    }
}
impl io::Read for PageReader<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.offset == self.bytes.len() && !self.ended {
            self.load().ok_or_else(corrupt_page)?;
        }
        if self.ended {
            return Ok(0);
        }
        let count = output.len().min(self.bytes.len() - self.offset);
        output[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
        self.offset += count;
        self.remaining_bound -= count;
        Ok(count)
    }
}
fn corrupt_page() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid native metadata page")
}
fn storage_error(error: io::Error) -> PatchError {
    PatchError::io(PatchOperationContext::PersistManifest, RollbackStatus::NotRequired, error)
}
