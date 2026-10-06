//! Exact filesystem patch planning for a confirmed rewind preview.

#[cfg(test)]
mod tests;

use super::{
    CheckpointFileVersion, ControlError, ControlStore, Error, FinalFile, Generation,
    LineEndingPolicy, PatchOperation, PatchSet, Preimage, ProductRunService, RevisionNumber,
    UserCheckpoint, WorkbenchRewindDisposition, WorkbenchRewindPreview, WorkspacePath, patch_input,
    patch_mode, patch_preimage,
};

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
        let patch = self.restore_plan_format(store, query, checkpoint, preview, true)?;
        if expected_identity.is_some_and(|expected| {
            patch.as_ref().is_some_and(|patch| patch.identity().digest() != expected)
        }) && !store.checkpoint_uses_chunks(checkpoint.id())?
        {
            // Only an already prepared legacy transaction may retain the v1 identity. Fresh
            // restores always use the snapshot representation, including old checkpoint bodies.
            return self.restore_plan_format(store, query, checkpoint, preview, false);
        }
        Ok(patch)
    }

    fn restore_plan_format(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
        streamed: bool,
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
                patch_preimage(path.owned_postchange().ok_or(ControlError::InvalidInput)?);
            let operation = match path.checkpoint() {
                CheckpointFileVersion::Absent => {
                    patch_input(PatchOperation::delete(workspace_path, preimage))?
                }
                target @ CheckpointFileVersion::Present { .. } if streamed => {
                    let snapshot = store
                        .checkpoint_snapshot(checkpoint.id(), index, target)?
                        .ok_or(Error::Corrupt("present checkpoint has no snapshot"))?;
                    match preimage {
                        Preimage::Absent => {
                            PatchOperation::create_snapshot(workspace_path, snapshot)
                        }
                        Preimage::Present { .. } => patch_input(PatchOperation::replace_snapshot(
                            workspace_path,
                            preimage,
                            snapshot,
                        ))?,
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
                        Preimage::Present { .. } => patch_input(PatchOperation::replace(
                            workspace_path,
                            preimage,
                            final_file,
                        ))?,
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
