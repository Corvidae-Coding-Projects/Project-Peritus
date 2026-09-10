//! Bounded selected-path capture and no-follow folder observations.

use super::{
    BTreeSet, CapturedCoverage, CapturedPath, CheckpointFileMode, CheckpointFileVersion,
    CheckpointId, CheckpointPath, ControlError, ControlIntent, ControlOperation, ConversationId,
    ConversationRecord, Error, FileReadSelection, FolderIdentity, FolderInspection, OperationId,
    Path, ProductRunService, UserCheckpoint, WorkspacePath, checkpoint_references,
    external_effects, fs, io, patch_input,
};
use peritus_product_runner::WorkspaceMutationKind;
use peritus_types::{ActorId, RunId, WorkspaceId};

const AUTOMATIC_CHECKPOINT_NAME: &str = "Automatic checkpoint before owned mutation";
const EMPTY_DIRECTORY_EXCLUSION: &str = "empty directory removal cannot restore the directory";

mod automatic;

impl ProductRunService {
    pub(super) fn capture_selected_coverage(
        &self,
        record: &ConversationRecord,
        query: peritus_app_protocol::WorkbenchQuery,
    ) -> Result<CapturedCoverage, Error> {
        let root = self.workspace_root(query)?;
        let identity = self.checked_folder_identity(query, root)?;
        let protected = self.protected_paths(query)?;
        let contract = record.inputs().capture()?.conversation().to_owned();
        let mut selected = BTreeSet::new();
        let mut exclusions = Vec::new();
        for entry in record.files().entries() {
            let source = entry.file().source();
            let reason = if !entry.selected() {
                Some("deselected")
            } else if source.path().is_none() {
                Some("external import has no workspace target")
            } else if source.range() != peritus_product_runner::control::FileRange::All {
                Some("partial selection is not a whole-file restore target")
            } else if source.folder() != Some(identity.digest()) {
                Some("folder identity differs from the current workspace")
            } else {
                None
            };
            if let Some(reason) = reason {
                push_exclusion(&mut exclusions, source.label(), reason)?;
                continue;
            }
            let path = source.path().ok_or(ControlError::InvalidInput)?.to_owned();
            if !selected.insert(path.clone()) {
                push_exclusion(&mut exclusions, &path, "duplicate reference already covered")?;
            }
        }
        if selected.len() > peritus_product_runner::control::MAX_CHECKPOINT_PATHS {
            return Err(ControlError::Capacity.into());
        }
        let mut paths = Vec::with_capacity(selected.len());
        let mut total = 0_usize;
        for path in &selected {
            check_protected(root, path, &contract, &protected)?;
            let captured = observe_path(&identity, path)?;
            total = total
                .checked_add(captured.body.as_ref().map_or(0, Vec::len))
                .ok_or(ControlError::Capacity)?;
            if total > peritus_patch::MAX_PATCH_BYTES {
                return Err(ControlError::Capacity.into());
            }
            paths.push(captured);
        }
        Ok(CapturedCoverage { paths, exclusions })
    }

    pub(super) fn capture_checkpoint_paths(
        &self,
        record: &ConversationRecord,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
    ) -> Result<Vec<CapturedPath>, Error> {
        let root = self.workspace_root(query)?;
        let identity = self.checked_folder_identity(query, root)?;
        let protected = self.protected_paths(query)?;
        let contract = record.inputs().capture()?.conversation().to_owned();
        let mut paths = Vec::with_capacity(checkpoint.paths().len());
        let mut total = 0_usize;
        for path in checkpoint.paths() {
            check_protected(root, path.path(), &contract, &protected)?;
            let captured = observe_path(&identity, path.path())?;
            total = total
                .checked_add(captured.body.as_ref().map_or(0, Vec::len))
                .ok_or(ControlError::Capacity)?;
            if total > peritus_patch::MAX_PATCH_BYTES {
                return Err(ControlError::Capacity.into());
            }
            paths.push(captured);
        }
        Ok(paths)
    }

    pub(super) fn workspace_root(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
    ) -> Result<&Path, Error> {
        self.inner
            .workspaces
            .get(&query.workspace())
            .map(std::path::PathBuf::as_path)
            .ok_or_else(|| ControlError::ScopeMismatch.into())
    }

    pub(super) fn checked_folder_identity(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
        root: &Path,
    ) -> Result<FolderIdentity, Error> {
        if let Some(folder) = self.inner.folders.get(&query.workspace()) {
            folder.verify().map_err(|_| ControlError::ScopeMismatch)?;
        }
        FolderIdentity::observe(root).map_err(Error::from)
    }

    pub(super) fn protected_paths(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
    ) -> Result<Vec<std::path::PathBuf>, Error> {
        let mut protected = vec![
            self.inner
                .directory
                .parent()
                .ok_or(Error::Corrupt("product run state parent missing"))?
                .to_path_buf(),
        ];
        if let Some(folder) = self.inner.folders.get(&query.workspace()) {
            protected.extend_from_slice(folder.protected_paths());
        }
        Ok(protected)
    }
}

fn validate_run_binding(start: &ControlOperation, run: RunId) -> Result<(), Error> {
    let expected = match start.intent() {
        ControlIntent::StartExecution { run, .. } | ControlIntent::StartGoal { run, .. } => run,
        _ => return Err(ControlError::InvalidInput.into()),
    };
    if expected != run.as_bytes() {
        return Err(ControlError::ScopeMismatch.into());
    }
    Ok(())
}

fn check_automatic_record(
    record: &ConversationRecord,
    actor: ActorId,
    workspace: WorkspaceId,
) -> Result<(), Error> {
    if record.owner_bytes() != actor.as_bytes() || record.workspace_bytes() != workspace.as_bytes()
    {
        return Err(ControlError::ScopeMismatch.into());
    }
    Ok(())
}

fn public_query(
    conversation: ConversationId,
    workspace: WorkspaceId,
) -> Result<peritus_app_protocol::WorkbenchQuery, Error> {
    Ok(peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new(*conversation.as_bytes())
            .map_err(|_| ControlError::InvalidInput)?,
        workspace,
    ))
}

fn automatic_checkpoint_id(
    run: RunId,
    path: &str,
    kind: WorkspaceMutationKind,
) -> Result<CheckpointId, Error> {
    let mut bytes = b"peritus-workbench-automatic-checkpoint-v1\0".to_vec();
    bytes.extend_from_slice(run.as_bytes());
    bytes.push(match kind {
        WorkspaceMutationKind::File => 1,
        WorkspaceMutationKind::EmptyDirectory => 2,
    });
    bytes.extend_from_slice(path.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    CheckpointId::new(id).map_err(Into::into)
}

fn validate_automatic_checkpoint(
    checkpoint: &UserCheckpoint,
    run: RunId,
    path: &str,
    kind: WorkspaceMutationKind,
) -> Result<(), Error> {
    let expected_id = automatic_checkpoint_id(run, path, kind)?;
    let expected_exclusions = match kind {
        WorkspaceMutationKind::File => Vec::new(),
        WorkspaceMutationKind::EmptyDirectory => vec![empty_directory_exclusion(path)],
    };
    let expected_path = match kind {
        WorkspaceMutationKind::File => Some(path),
        WorkspaceMutationKind::EmptyDirectory => None,
    };
    let exact = checkpoint.id() == expected_id
        && checkpoint.name() == AUTOMATIC_CHECKPOINT_NAME
        && checkpoint.automatic_run() == Some(run.into_bytes())
        && checkpoint.paths().len() == usize::from(expected_path.is_some())
        && checkpoint.paths().first().map(CheckpointPath::path) == expected_path
        && checkpoint.exclusions().eq(expected_exclusions.iter().map(String::as_str));
    if exact { Ok(()) } else { Err(ControlError::IdempotencyConflict.into()) }
}

fn empty_directory_exclusion(path: &str) -> String {
    format!("{path}: {EMPTY_DIRECTORY_EXCLUSION}")
}

fn observe_empty_directory(identity: &FolderIdentity, path: &str) -> Result<(), Error> {
    let relative = patch_input(WorkspacePath::new(path))?;
    let metadata = safe_metadata(identity.root(), &relative)?.ok_or(ControlError::StaleRevision)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ControlError::InvalidInput.into());
    }
    if fs::read_dir(identity.root().join(path))?.next().transpose()?.is_some() {
        return Err(ControlError::StaleRevision.into());
    }
    Ok(())
}

pub(super) fn observe_path(identity: &FolderIdentity, path: &str) -> Result<CapturedPath, Error> {
    let relative = patch_input(WorkspacePath::new(path))?;
    let target = identity.root().join(path);
    let before = match safe_metadata(identity.root(), &relative)? {
        Some(metadata) => metadata,
        None => {
            return Ok(CapturedPath {
                path: path.to_owned(),
                version: CheckpointFileVersion::Absent,
                body: None,
            });
        }
    };
    if !before.is_file() || before.file_type().is_symlink() {
        return Err(ControlError::InvalidInput.into());
    }
    let inspected = FolderInspection::open(identity)
        .and_then(|reader| {
            reader.read_file(
                &relative,
                FileReadSelection::all(),
                peritus_patch::MAX_FILE_BYTES as u64,
            )
        })
        .map_err(|_| ControlError::StaleRevision)?;
    let after = fs::symlink_metadata(&target)?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || !same_metadata(&before, &after)
        || inspected.source_bytes() != after.len()
    {
        return Err(ControlError::StaleRevision.into());
    }
    let mode = file_mode(&after);
    let body = inspected.bytes().to_vec();
    let version = CheckpointFileVersion::present(inspected.source_digest(), after.len(), mode);
    Ok(CapturedPath { path: path.to_owned(), version, body: Some(body) })
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
        same
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

pub(super) fn check_protected(
    root: &Path,
    path: &str,
    contract: &str,
    protected: &[std::path::PathBuf],
) -> Result<(), Error> {
    peritus_product_runner::checked_protected_file(root, path, contract, protected)
        .map(|_| ())
        .map_err(|_| ControlError::InvalidInput.into())
}

pub(super) fn push_exclusion(
    values: &mut Vec<String>,
    label: &str,
    reason: &str,
) -> Result<(), Error> {
    if values.len() >= peritus_product_runner::control::MAX_CHECKPOINT_PATHS {
        return Err(ControlError::Capacity.into());
    }
    values.push(format!("{label}: {reason}"));
    Ok(())
}
