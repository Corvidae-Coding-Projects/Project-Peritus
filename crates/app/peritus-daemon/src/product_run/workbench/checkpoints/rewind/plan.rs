//! Exact filesystem patch planning for a confirmed rewind preview.

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
            self.restore_plan_with_store(store, query, checkpoint, preview)
        })
    }

    pub(in crate::product_run::workbench::checkpoints) fn restore_plan_with_store(
        &self,
        store: &ControlStore,
        query: peritus_app_protocol::WorkbenchQuery,
        checkpoint: &UserCheckpoint,
        preview: &WorkbenchRewindPreview,
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
        let patch = patch_input(PatchSet::new(
            query.workspace(),
            Generation::first(),
            RevisionNumber::new(preview.request().revision())
                .map_err(|_| ControlError::InvalidInput)?,
            operations,
        ))?;
        Ok(Some(patch))
    }
}
