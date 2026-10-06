//! Exact filesystem patch planning for a confirmed rewind preview.

mod ranges;
#[cfg(test)]
mod tests;

use super::{
    CheckpointFileVersion, ControlError, ControlStore, Error, FinalFile, Generation,
    LineEndingPolicy, PatchOperation, PatchSet, Preimage, ProductRunService, RevisionNumber,
    UserCheckpoint, WorkbenchRewindDisposition, WorkbenchRewindPreview, WorkspacePath, patch_input,
    patch_mode, patch_preimage,
};

pub(in crate::product_run::workbench::checkpoints) fn selected_coverage_matches(
    store: &ControlStore,
    checkpoint: super::CheckpointId,
    index: usize,
    expected: &super::CheckpointPath,
    current: &super::super::CapturedPath,
) -> Result<(), Error> {
    let peritus_product_runner::control::CheckpointCoverage::SelectedRanges(selected) =
        expected.coverage()
    else {
        return Err(ControlError::InvalidInput.into());
    };
    let saved = store
        .checkpoint_snapshot(checkpoint, index, expected.checkpoint())?
        .ok_or(Error::Corrupt("selected fork source missing"))?;
    let (Some(body), Some(digest), Some(bytes), Some(mode)) =
        (&current.body, current.version.digest(), current.version.bytes(), current.version.mode())
    else {
        return Err(Error::StalePreimage);
    };
    let current = peritus_patch::SnapshotFile::new(
        std::fs::File::open(body)?,
        digest,
        bytes,
        patch_mode(mode),
    );
    match ranges::materialize(&saved, &current, selected) {
        Ok(restored) if restored.identity() == current.identity() => Ok(()),
        Ok(_) | Err(Error::Control(ControlError::InvalidInput)) => Err(Error::StalePreimage),
        Err(error) => Err(error),
    }
}

impl ProductRunService {
    pub(super) fn restore_plan(
        &self,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
    ) -> Result<Option<PatchSet>, Error> {
        self.with_controls(false, |store| {
            self.restore_plan_with_store(store, query, checkpoint, preview, None)
        })
    }

    pub(in crate::product_run::workbench::checkpoints) fn restore_plan_with_store(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
        expected_identity: Option<peritus_types::Sha256Digest>,
    ) -> Result<Option<PatchSet>, Error> {
        self.restore_plan_materialized(store, query, checkpoint, preview, expected_identity, None)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "exact capture, preview and durable recovery bindings stay explicit"
    )]
    pub(in crate::product_run::workbench::checkpoints) fn restore_plan_materialized(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
        expected_identity: Option<peritus_types::Sha256Digest>,
        range_input: Option<(&UserCheckpoint, Option<&[Option<tempfile::TempPath>]>)>,
    ) -> Result<Option<PatchSet>, Error> {
        let patch =
            self.restore_plan_format(store, query, checkpoint, preview, true, range_input)?;
        if expected_identity.is_some_and(|expected| {
            patch.as_ref().is_some_and(|patch| patch.identity().digest() != expected)
        }) && !store.checkpoint_uses_chunks(checkpoint.id())?
        {
            // Only an already prepared legacy transaction may retain the v1 identity. Fresh
            // restores always use the snapshot representation, including old checkpoint bodies.
            return self.restore_plan_format(store, query, checkpoint, preview, false, range_input);
        }
        Ok(patch)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "legacy representation and exact range recovery input are independent"
    )]
    fn restore_plan_format(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
        streamed: bool,
        range_input: Option<(&UserCheckpoint, Option<&[Option<tempfile::TempPath>]>)>,
    ) -> Result<Option<PatchSet>, Error> {
        let mut operations = Vec::new();
        for (index, (path, preview_path)) in
            checkpoint.paths().iter().zip(preview.paths()).enumerate()
        {
            if preview_path.disposition() != WorkbenchRewindDisposition::Restore {
                continue;
            }
            let workspace_path = patch_input(WorkspacePath::new(path.path()))?;
            let preimage =
                patch_preimage(path.owned_postchange().ok_or(ControlError::InvalidInput)?)?;
            if let peritus_product_runner::control::CheckpointCoverage::SelectedRanges(selected) =
                path.coverage()
            {
                if !streamed {
                    return Err(Error::Corrupt(
                        "selected-range restores require snapshot representation",
                    ));
                }
                let (recovery, bodies) = range_input
                    .ok_or(Error::Corrupt("selected-range restore has no exact recovery input"))?;
                let before = recovery.paths().get(index).ok_or(ControlError::InvalidInput)?;
                if before.path() != path.path() || patch_preimage(before.checkpoint())? != preimage
                {
                    return Err(Error::StalePreimage);
                }
                let captured = store
                    .checkpoint_snapshot(checkpoint.id(), index, path.checkpoint())?
                    .ok_or(Error::Corrupt("selected-range source snapshot missing"))?;
                let current = match bodies {
                    Some(bodies) => {
                        let body = bodies
                            .get(index)
                            .and_then(Option::as_ref)
                            .ok_or(ControlError::InvalidInput)?;
                        peritus_patch::SnapshotFile::new(
                            std::fs::File::open(body)?,
                            before.checkpoint().digest().ok_or(ControlError::InvalidInput)?,
                            before.checkpoint().bytes().ok_or(ControlError::InvalidInput)?,
                            patch_mode(
                                before.checkpoint().mode().ok_or(ControlError::InvalidInput)?,
                            ),
                        )
                    }
                    None => store
                        .checkpoint_snapshot(recovery.id(), index, before.checkpoint())?
                        .ok_or(Error::Corrupt("range recovery snapshot missing"))?,
                };
                let snapshot = ranges::materialize(&captured, &current, selected)?;
                operations.push(patch_input(PatchOperation::replace_snapshot(
                    workspace_path,
                    preimage,
                    snapshot,
                ))?);
                continue;
            }
            let operation = match path.checkpoint() {
                CheckpointFileVersion::Absent => match preimage {
                    Preimage::EmptyDirectory { mode } => {
                        PatchOperation::delete_directory(workspace_path, mode)
                    }
                    _ => patch_input(PatchOperation::delete(workspace_path, preimage))?,
                },
                CheckpointFileVersion::EmptyDirectory { permissions } => {
                    let mode = patch_input(peritus_patch::DirectoryMode::new(permissions))?;
                    if preimage == Preimage::Absent {
                        PatchOperation::create_directory(workspace_path, mode)
                    } else {
                        patch_input(PatchOperation::replace_directory(
                            workspace_path,
                            preimage,
                            mode,
                        ))?
                    }
                }
                target @ CheckpointFileVersion::Present { .. } if streamed => {
                    let snapshot = store
                        .checkpoint_snapshot(checkpoint.id(), index, target)?
                        .ok_or(Error::Corrupt("present checkpoint has no snapshot"))?;
                    match preimage {
                        Preimage::Absent => {
                            PatchOperation::create_snapshot(workspace_path, snapshot)
                        }
                        Preimage::Present { .. } | Preimage::EmptyDirectory { .. } => patch_input(
                            PatchOperation::replace_snapshot(workspace_path, preimage, snapshot),
                        )?,
                    }
                }
                target @ CheckpointFileVersion::Present { .. } => {
                    let body = store
                        .checkpoint_body(checkpoint.id(), index, target)?
                        .ok_or(Error::Corrupt("present checkpoint target has no retained bytes"))?;
                    let final_file = patch_input(FinalFile::new(
                        body,
                        patch_mode(target.mode().ok_or(ControlError::InvalidInput)?),
                        LineEndingPolicy::Preserve,
                    ))?;
                    match preimage {
                        Preimage::Absent => PatchOperation::create(workspace_path, final_file),
                        Preimage::Present { .. } | Preimage::EmptyDirectory { .. } => patch_input(
                            PatchOperation::replace(workspace_path, preimage, final_file),
                        )?,
                    }
                }
            };
            operations.push(operation);
        }
        if operations.is_empty() {
            return Ok(None);
        }
        let constructor = if streamed { PatchSet::from_snapshot } else { PatchSet::new };
        let patch = patch_input(constructor(
            query.workspace(),
            Generation::first(),
            RevisionNumber::new(preview.request().revision())
                .map_err(|_| ControlError::InvalidInput)?,
            operations,
        ))?;
        Ok(Some(patch))
    }
}
