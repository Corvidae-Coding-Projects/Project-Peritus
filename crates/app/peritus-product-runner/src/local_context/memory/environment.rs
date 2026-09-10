//! Exact host workspace/conversation bindings, with separately observed file dependencies.

use super::super::error;
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextNodeId,
    working::{WorkingBinding, WorkingEnvironment, WorkingFileDigest, WorkingLimits},
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::Read as _,
    path::{Path, PathBuf},
};

/// Direct folders have no whole-tree candidate identity. Only explicit file dependencies,
/// conversation and task validity are supported; the digest is a directory namespace only.
#[derive(Clone, Default)]
pub(in crate::local_context) struct WorkspaceScope {
    pub(in crate::local_context) direct: bool,
    pub(in crate::local_context) protected: Vec<PathBuf>,
}

pub(in crate::local_context) fn key(label: &[u8]) -> Result<ContextNodeId, DeveloperLoopError> {
    let digest = sha256(label);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    ContextNodeId::new(id).map_err(|_| error("invalid derived context identity"))
}

pub(in crate::local_context) fn capture(
    root: &Path,
    binding: WorkingBinding,
    paths: &[String],
    contract: &str,
    limits: WorkingLimits,
    scope: &WorkspaceScope,
) -> Result<WorkingEnvironment, DeveloperLoopError> {
    let candidate = if scope.direct {
        sha256(&binding.workspace().into_bytes())
    } else {
        crate::progress::WorkspaceCheckpoint::capture(root)
            .map_err(|_| error("capture candidate identity"))?
            .digest()
    };
    let mut files = Vec::new();
    for path in paths {
        let Ok(full) =
            crate::developer_tools::checked_protected_file(root, path, contract, &scope.protected)
        else {
            continue;
        };
        match File::open(full) {
            Ok(file) => {
                files.push(WorkingFileDigest::new(key(path.as_bytes())?, digest_file(file)?));
            }
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(error("inspect file dependency")),
        }
    }
    files.sort_by_key(|file| file.key());
    files.dedup();
    WorkingEnvironment::new(binding, candidate, files, limits)
        .map_err(|_| error("invalid working environment"))
}

fn digest_file(mut file: File) -> Result<Sha256Digest, DeveloperLoopError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    let mut total = 0_u64;
    loop {
        let count = file.read(&mut buffer).map_err(|_| error("hash file dependency"))?;
        if count == 0 {
            return Ok(Sha256Digest::new(hasher.finalize().into()));
        }
        total = total.checked_add(count as u64).ok_or_else(|| error("file size overflow"))?;
        if total > 64 * 1024 * 1024 {
            return Err(error("file dependency exceeds inspection bound"));
        }
        hasher.update(&buffer[..count]);
    }
}
