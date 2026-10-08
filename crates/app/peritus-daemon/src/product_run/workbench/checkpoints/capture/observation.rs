//! Complete streamed file and typed empty-directory observations within one folder identity.

use super::{
    CapturedPath, CheckpointFileMode, CheckpointFileVersion, ControlError, Error, FolderIdentity,
    FolderInspection, Path, WorkspaceMutationBaseline, WorkspaceMutationKind, WorkspacePath, fs,
    io, patch_input, patch_preimage,
};

pub(super) fn capture_baseline(
    path: &str,
    kind: WorkspaceMutationKind,
    baseline: &WorkspaceMutationBaseline,
) -> Result<CapturedPath, Error> {
    patch_input(WorkspacePath::new(path))?;
    let version = baseline.version();
    let body = match (kind, version, baseline.snapshot()) {
        (WorkspaceMutationKind::File, CheckpointFileVersion::Absent, None)
        | (
            WorkspaceMutationKind::EmptyDirectory,
            CheckpointFileVersion::EmptyDirectory { .. },
            None,
        ) => None,
        (
            WorkspaceMutationKind::File,
            version @ CheckpointFileVersion::Present { .. },
            Some(snapshot),
        ) => {
            if snapshot.identity() != patch_preimage(version)? {
                return Err(ControlError::InvalidInput.into());
            }
            let mut staged = tempfile::NamedTempFile::new()?;
            patch_input(snapshot.write_to(&mut staged))?;
            staged.as_file().sync_all()?;
            Some(staged.into_temp_path())
        }
        _ => return Err(ControlError::InvalidInput.into()),
    };
    Ok(CapturedPath { path: path.to_owned(), version, body, ranges: Vec::new() })
}

pub(super) fn observe_empty_directory(
    identity: &FolderIdentity,
    path: &str,
) -> Result<CheckpointFileVersion, Error> {
    let relative = patch_input(WorkspacePath::new(path))?;
    let metadata = safe_metadata(identity.root(), &relative)?.ok_or(Error::StalePreimage)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ControlError::InvalidInput.into());
    }
    let mode = FolderInspection::open(identity)?.empty_directory_mode(&relative)?;
    let after = fs::symlink_metadata(identity.root().join(path))?;
    if !same_metadata(&metadata, &after) {
        return Err(Error::StalePreimage);
    }
    Ok(CheckpointFileVersion::empty_directory(mode))
}

pub(in crate::product_run::workbench::checkpoints) fn observe_path(
    identity: &FolderIdentity,
    path: &str,
) -> Result<CapturedPath, Error> {
    observe_file(identity, path, true)
}

pub(in crate::product_run::workbench::checkpoints) fn observe_version(
    identity: &FolderIdentity,
    path: &str,
) -> Result<CheckpointFileVersion, Error> {
    observe_file(identity, path, false).map(|captured| captured.version)
}

pub(super) fn observe_file(
    identity: &FolderIdentity,
    path: &str,
    retain: bool,
) -> Result<CapturedPath, Error> {
    let relative = patch_input(WorkspacePath::new(path))?;
    let target = identity.root().join(path);
    let before = match safe_metadata(identity.root(), &relative)? {
        Some(metadata) => metadata,
        None => {
            return Ok(CapturedPath {
                path: path.to_owned(),
                version: CheckpointFileVersion::Absent,
                body: None,
                ranges: Vec::new(),
            });
        }
    };
    if before.is_dir() && !before.file_type().is_symlink() {
        return Ok(CapturedPath {
            path: path.to_owned(),
            version: observe_empty_directory(identity, path)?,
            body: None,
            ranges: Vec::new(),
        });
    }
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(ControlError::InvalidInput.into());
    }
    let mut body = retain.then(tempfile::NamedTempFile::new).transpose()?;
    let inspection = FolderInspection::open(identity)?;
    let (digest, bytes) = match body.as_mut() {
        Some(body) => inspection.copy_snapshot(&relative, body)?,
        None => inspection.copy_snapshot(&relative, &mut io::sink())?,
    };
    let after = fs::symlink_metadata(&target)?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || !same_metadata(&before, &after)
        || bytes != after.len()
    {
        return Err(Error::StalePreimage);
    }
    let mode = file_mode(&after);
    let version = CheckpointFileVersion::present(digest, after.len(), mode);
    Ok(CapturedPath {
        path: path.to_owned(),
        version,
        body: body.map(tempfile::NamedTempFile::into_temp_path),
        ranges: Vec::new(),
    })
}

fn safe_metadata(root: &Path, path: &WorkspacePath) -> Result<Option<fs::Metadata>, Error> {
    let mut current = root.to_path_buf();
    let components = path.as_str().split('/').collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || (index + 1 < components.len() && !metadata.is_dir())
                {
                    return Err(ControlError::InvalidInput.into());
                }
                if index + 1 == components.len() {
                    return Ok(Some(metadata));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    Err(ControlError::InvalidInput.into())
}

fn same_metadata(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    let same = left.len() == right.len() && left.modified().ok() == right.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        same && left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
            && left.mode() == right.mode()
    }
    #[cfg(not(unix))]
    {
        same && left.permissions().readonly() == right.permissions().readonly()
    }
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> CheckpointFileMode {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o111 == 0 {
        CheckpointFileMode::Regular
    } else {
        CheckpointFileMode::Executable
    }
}

#[cfg(not(unix))]
const fn file_mode(_metadata: &fs::Metadata) -> CheckpointFileMode {
    CheckpointFileMode::Regular
}
