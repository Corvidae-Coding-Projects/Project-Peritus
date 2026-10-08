//! Exact host workspace/conversation bindings, with separately observed file dependencies.

use super::super::error;
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_context::{
    ContextNodeId,
    working::{WorkingBinding, WorkingEnvironment, WorkingFileDigest, WorkingLimits, WorkingState},
};
use peritus_patch::WorkspacePath;
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;
use peritus_workspace::{ErrorCode as WorkspaceErrorCode, FolderIdentity, FolderInspection};
use std::path::{Path, PathBuf};

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
    cancellation: &CancellationToken,
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
        if crate::developer_tools::checked_context_file_for_developer(
            root,
            path,
            contract,
            &scope.protected,
        )?
        .is_none()
        {
            continue;
        }
        if let Some(receipt) = digest_file(root, path, cancellation)? {
            let FileDigestReceipt { digest, bytes: _ } = receipt;
            files.push(WorkingFileDigest::new(key(path.as_bytes())?, digest));
        }
    }
    files.sort_by_key(|file| file.key());
    files.dedup();
    WorkingEnvironment::new(binding, candidate, files, limits)
        .map_err(|_| error("invalid working environment"))
}

pub(in crate::local_context) fn projected_paths(
    state: &WorkingState,
    archive: &[String],
    additions: &[String],
) -> Result<Vec<String>, DeveloperLoopError> {
    let entries = state
        .active_entries(state.binding())
        .map_err(|_| error("working-state file projection binding mismatch"))?;
    let mut keys = entries
        .iter()
        .flat_map(|entry| entry.validity().files().iter().map(|file| file.key()))
        .collect::<Vec<_>>();
    for path in additions {
        keys.push(key(path.as_bytes())?);
    }
    keys.sort();
    keys.dedup();
    archive
        .iter()
        .filter_map(|path| match key(path.as_bytes()) {
            Ok(path_key) if keys.binary_search(&path_key).is_ok() => Some(Ok(path.clone())),
            Ok(_) => None,
            Err(reason) => Some(Err(reason)),
        })
        .collect()
}

struct FileDigestReceipt {
    digest: Sha256Digest,
    bytes: u64,
}

struct InspectionProgress<'a> {
    cancellation: &'a CancellationToken,
    bytes: u64,
}

impl std::io::Write for InspectionProgress<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "file dependency inspection cancelled",
            ));
        }
        let count = u64::try_from(bytes.len())
            .map_err(|_| std::io::Error::other("file dependency byte count overflowed"))?;
        self.bytes = self
            .bytes
            .checked_add(count)
            .ok_or_else(|| std::io::Error::other("file dependency byte count overflowed"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.cancellation.is_cancelled() {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "file dependency inspection cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

fn digest_file(
    root: &Path,
    path: &str,
    cancellation: &CancellationToken,
) -> Result<Option<FileDigestReceipt>, DeveloperLoopError> {
    if cancellation.is_cancelled() {
        return Err(error("hash file dependency cancelled"));
    }
    let selected = WorkspacePath::new(path).map_err(|_| error("invalid file dependency path"))?;
    let identity = FolderIdentity::observe(root).map_err(|_| error("observe workspace identity"))?;
    let inspection =
        FolderInspection::open(&identity).map_err(|_| error("open file dependency inspection"))?;
    let mut progress = InspectionProgress { cancellation, bytes: 0 };
    match inspection.copy_snapshot(&selected, &mut progress) {
        Ok((digest, bytes)) if !cancellation.is_cancelled() && progress.bytes == bytes => {
            Ok(Some(FileDigestReceipt { digest, bytes }))
        }
        Ok(_) if cancellation.is_cancelled() => Err(error("hash file dependency cancelled")),
        Ok(_) => Err(error("file dependency progress receipt mismatch")),
        Err(_) if cancellation.is_cancelled() => Err(error("hash file dependency cancelled")),
        Err(failure) if missing_file(&failure) => Ok(None),
        Err(failure)
            if failure.code() == WorkspaceErrorCode::Indeterminate
                && failure.detail() == "source identity or content changed during inspection" =>
        {
            Err(error("file dependency changed during inspection"))
        }
        Err(_) => Err(error("inspect file dependency")),
    }
}

fn missing_file(failure: &peritus_workspace::WorkspaceError) -> bool {
    std::error::Error::source(failure)
        .and_then(|source| source.downcast_ref::<std::io::Error>())
        .is_some_and(|source| source.kind() == std::io::ErrorKind::NotFound)
}
