//! Bounded, checksummed restore intent and progress, separate from source effects.

use super::{ManagedBaseline, ProductRunnerError, failure, owned::Asset, validation::RootFact};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 64 * 1024 * 1024;
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
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + HEADER + 1) as u64).read_to_end(&mut bytes).map_err(failure)?;
    if bytes.len() < HEADER || bytes.len() > MAX_BYTES + HEADER || &bytes[..4] != MAGIC {
        return Err(failure("discard intent is missing, oversized, or corrupt"));
    }
    let declared = u32::from_be_bytes(bytes[4..8].try_into().map_err(failure)?) as usize;
    if declared != bytes.len() - HEADER
        || Sha256::digest(&bytes[HEADER..]).as_slice() != &bytes[8..HEADER]
    {
        return Err(failure("discard intent checksum or length is invalid"));
    }
    serde_json::from_slice(&bytes[HEADER..]).map(Some).map_err(failure)
}

pub(super) fn save(path: &Path, state: &State) -> Result<(), ProductRunnerError> {
    ordinary_metadata(path)?;
    let payload = serde_json::to_vec(state).map_err(failure)?;
    if payload.len() > MAX_BYTES {
        return Err(failure("discard intent exceeds its size limit"));
    }
    let mut bytes = Vec::with_capacity(HEADER + payload.len());
    bytes.extend(MAGIC);
    bytes.extend(u32::try_from(payload.len()).map_err(failure)?.to_be_bytes());
    bytes.extend(Sha256::digest(&payload));
    bytes.extend(payload);
    let parent = path.parent().ok_or_else(|| failure("discard intent has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(failure)?;
    temporary.write_all(&bytes).and_then(|()| temporary.as_file().sync_all()).map_err(failure)?;
    temporary.persist(path).map_err(failure)?;
    super::super::recovery::sync_directory(parent)
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
