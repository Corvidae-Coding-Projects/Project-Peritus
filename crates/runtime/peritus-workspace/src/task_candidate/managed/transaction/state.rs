//! Bounded, checksummed restore intent and progress, separate from source effects.

use super::{ManagedBaseline, ProductRunnerError, failure, owned::Asset, validation::RootFact};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 4] = b"P5DS";
const HEADER: usize = 40;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub version: u8,
    pub binding: [u8; 32],
    pub root: PathBuf,
    pub candidate: [u8; 32],
    pub head: String,
    pub baseline: ManagedBaseline,
    pub before: ManagedBaseline,
    pub roots: BTreeMap<PathBuf, RootFact>,
    pub owners: BTreeSet<PathBuf>,
    pub paths: Vec<PathBuf>,
}

impl Plan {
    pub fn digest(&self) -> Result<[u8; 32], ProductRunnerError> {
        Ok(Sha256::digest(serde_json::to_vec(self).map_err(failure)?).into())
    }

    pub fn validate(&self, binding: [u8; 32]) -> Result<(), ProductRunnerError> {
        if self.version != 1
            || self.binding != binding
            || self.root.canonicalize().map_err(failure)? != self.root
        {
            return Err(failure("discard intent belongs to another workspace or operation"));
        }
        self.baseline.validate()?;
        self.before.validate()?;
        let unique = self.paths.iter().collect::<BTreeSet<_>>();
        if unique.len() != self.paths.len() {
            return Err(failure("discard intent has invalid owned paths"));
        }
        for path in &self.paths {
            super::super::validate_path(path)?;
        }
        super::validation::structure(self)?;
        if super::validation::candidate_digest(&self.before, &self.head)? != self.candidate {
            return Err(failure("discard preimages do not match the admitted candidate"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub(super) enum Phase {
    Prepared,
    Restoring,
    Completed,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recovery {
    pub path: PathBuf,
    pub source: Option<PathBuf>,
    pub directory_digest: Option<[u8; 32]>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub plan: Plan,
    pub phase: Phase,
    pub assets: Vec<Asset>,
    pub heads: BTreeMap<PathBuf, RootFact>,
    pub recovery: Vec<Recovery>,
}

pub(super) fn read(path: &Path) -> Result<Option<State>, ProductRunnerError> {
    if ordinary_metadata(path)?.is_none() {
        return Ok(None);
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failure(error)),
    };
    let mut file = file;
    let mut header = [0_u8; HEADER];
    file.read_exact(&mut header).map_err(failure)?;
    if &header[..4] != MAGIC {
        return Err(failure("discard intent is missing, oversized, or corrupt"));
    }
    let declared = u32::from_be_bytes(header[4..8].try_into().map_err(failure)?) as usize;
    let mut payload = Vec::new();
    payload.try_reserve_exact(declared).map_err(failure)?;
    let mut remaining = declared;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut checksum = Sha256::new();
    while remaining > 0 {
        let count = remaining.min(buffer.len());
        file.read_exact(&mut buffer[..count]).map_err(failure)?;
        checksum.update(&buffer[..count]);
        payload.extend_from_slice(&buffer[..count]);
        remaining -= count;
    }
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing).map_err(failure)? != 0
        || checksum.finalize().as_slice() != &header[8..HEADER]
    {
        return Err(failure("discard intent checksum or length is invalid"));
    }
    serde_json::from_slice(&payload).map(Some).map_err(failure)
}

pub(super) fn save(path: &Path, state: &State) -> Result<(), ProductRunnerError> {
    ordinary_metadata(path)?;
    let parent = path.parent().ok_or_else(|| failure("discard intent has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(failure)?;
    temporary.as_file_mut().write_all(&[0_u8; HEADER]).map_err(failure)?;
    let (length, digest) = {
        let mut payload = PayloadWriter::new(temporary.as_file_mut());
        serde_json::to_writer(&mut payload, state).map_err(failure)?;
        payload.flush().map_err(failure)?;
        payload.finish()
    };
    let mut header = Vec::with_capacity(HEADER);
    header.extend(MAGIC);
    header.extend(length.to_be_bytes());
    header.extend(digest);
    temporary.as_file_mut().seek(SeekFrom::Start(0)).map_err(failure)?;
    temporary.as_file_mut().write_all(&header).map_err(failure)?;
    temporary.as_file().sync_all().map_err(failure)?;
    temporary.persist(path).map_err(failure)?;
    super::super::recovery::sync_directory(parent)
}

struct PayloadWriter<'a> {
    file: &'a mut fs::File,
    length: u32,
    digest: Sha256,
}

impl<'a> PayloadWriter<'a> {
    fn new(file: &'a mut fs::File) -> Self {
        Self { file, length: 0, digest: Sha256::new() }
    }

    fn finish(self) -> (u32, [u8; 32]) {
        (self.length, self.digest.finalize().into())
    }
}

impl std::io::Write for PayloadWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.file.write(bytes)?;
        let length = self
            .length
            .checked_add(u32::try_from(count).map_err(std::io::Error::other)?)
            .ok_or_else(|| std::io::Error::other("discard intent exceeds u32 payload format"))?;
        self.digest.update(&bytes[..count]);
        self.length = length;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

pub(super) fn ordinary_metadata(path: &Path) -> Result<Option<fs::Metadata>, ProductRunnerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Ok(Some(metadata))
        }
        Ok(_) => Err(failure("discard record is not an ordinary file; it was preserved")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(failure(error)),
    }
}
