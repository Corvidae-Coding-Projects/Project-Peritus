//! Streaming selected-path capture and no-follow folder observations.

use super::{
    CapturedCoverage, CapturedPath, CheckpointFileMode, CheckpointFileVersion, CheckpointId,
    CheckpointPath, ControlError, ControlIntent, ControlOperation, ConversationId,
    ConversationRecord, Error, FolderIdentity, FolderInspection, OperationId, Path,
    ProductRunService, UserCheckpoint, WorkspacePath, checkpoint_references, external_effects, fs,
    io, patch_input,
};
use peritus_product_runner::WorkspaceMutationKind;
use peritus_product_runner::control::{CheckpointRange, FileRange};
use peritus_types::{ActorId, RunId, WorkspaceId};
use std::collections::BTreeMap;

const AUTOMATIC_CHECKPOINT_NAME: &str = "Automatic checkpoint before owned mutation";
const EMPTY_DIRECTORY_EXCLUSION: &str = "empty directory removal cannot restore the directory";

mod automatic;
mod observation;
use observation::{observe_empty_directory, observe_file};
pub(super) use observation::{observe_path, observe_version};

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
        let mut selected: BTreeMap<String, Vec<FileRange>> = BTreeMap::new();
        let mut exclusions = Vec::new();
        for entry in record.files().entries() {
            let source = entry.file().source();
            let reason = if !entry.selected() {
                Some("deselected")
            } else if source.path().is_none() {
                Some("external import has no workspace target")
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
            selected.entry(path).or_default().push(source.range());
        }
        let mut paths = Vec::with_capacity(selected.len());
        for (path, selections) in &selected {
            check_protected(root, path, &contract, &protected)?;
            let mut captured = observe_path(&identity, path)?;
            if !selections.contains(&FileRange::All) {
                let body = captured.body.as_ref().ok_or(ControlError::InvalidInput)?;
                let intervals = super::ranges::resolve_ranges(
                    &mut fs::File::open(body)?,
                    selections,
                    captured.version.bytes().ok_or(ControlError::InvalidInput)?,
                )?;
                captured.ranges = selections
                    .iter()
                    .zip(intervals)
                    .map(|(selection, (start, end))| CheckpointRange::new(*selection, start, end))
                    .collect::<Result<_, _>>()?;
                // The checked constructor canonicalizes overlapping/duplicate descriptors
                // without discarding distinct selected ranges on the same workspace path.
                let manifest = captured.manifest()?;
                if let peritus_product_runner::control::CheckpointCoverage::SelectedRanges(ranges) =
                    manifest.coverage()
                {
                    captured.ranges = ranges.to_vec();
                }
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
        self.inspect_checkpoint_paths(record, query, checkpoint, true)
    }

    pub(super) fn observe_checkpoint_paths(
        &self,
        record: &ConversationRecord,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
    ) -> Result<Vec<CapturedPath>, Error> {
        self.inspect_checkpoint_paths(record, query, checkpoint, false)
    }

    fn inspect_checkpoint_paths(
        &self,
        record: &ConversationRecord,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        retain: bool,
    ) -> Result<Vec<CapturedPath>, Error> {
        let root = self.workspace_root(query)?;
        let identity = self.checked_folder_identity(query, root)?;
        let protected = self.protected_paths(query)?;
        let contract = record.inputs().capture()?.conversation().to_owned();
        let mut paths = Vec::with_capacity(checkpoint.paths().len());
        for path in checkpoint.paths() {
            check_protected(root, path.path(), &contract, &protected)?;
            let captured = observe_file(&identity, path.path(), retain)?;
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

    pub(in crate::product_run::workbench) fn protected_paths(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
    ) -> Result<Vec<std::path::PathBuf>, Error> {
        let root = self.workspace_root(query)?;
        let state_root = self
            .inner
            .directory
            .parent()
            .ok_or(Error::Corrupt("product run state parent missing"))?;
        // Managed workspaces live below daemon state. Their root is already the read/write
        // capability boundary; protecting its ancestor would incorrectly protect every file.
        let mut protected =
            if state_root.starts_with(root) { vec![state_root.to_path_buf()] } else { Vec::new() };
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
    // Previously accepted directory checkpoints retained only this explicit exclusion. Keep
    // their exact manifest usable on replay; all new captures retain a typed directory target.
    let legacy_directory = kind == WorkspaceMutationKind::EmptyDirectory
        && checkpoint.paths().is_empty()
        && checkpoint.exclusions().eq([empty_directory_exclusion(path).as_str()]);
    let expected_exclusions =
        if legacy_directory { vec![empty_directory_exclusion(path)] } else { Vec::new() };
    let expected_path = if legacy_directory { None } else { Some(path) };
    let exact = checkpoint.id() == expected_id
        && checkpoint.name() == AUTOMATIC_CHECKPOINT_NAME
        && checkpoint.automatic_run() == Some(run.into_bytes())
        && checkpoint.paths().len() == usize::from(expected_path.is_some())
        && checkpoint.paths().first().map(CheckpointPath::path) == expected_path
        && checkpoint.paths().iter().all(|target| {
            !matches!(
                target.coverage(),
                peritus_product_runner::control::CheckpointCoverage::SelectedRanges(_)
            ) && match kind {
                WorkspaceMutationKind::File => matches!(
                    target.checkpoint(),
                    CheckpointFileVersion::Absent | CheckpointFileVersion::Present { .. }
                ),
                WorkspaceMutationKind::EmptyDirectory => {
                    matches!(target.checkpoint(), CheckpointFileVersion::EmptyDirectory { .. })
                }
            }
        })
        && checkpoint.exclusions().eq(expected_exclusions.iter().map(String::as_str));
    if exact { Ok(()) } else { Err(ControlError::IdempotencyConflict.into()) }
}

fn empty_directory_exclusion(path: &str) -> String {
    format!("{path}: {EMPTY_DIRECTORY_EXCLUSION}")
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
    values.push(format!("{label}: {reason}"));
    Ok(())
}
