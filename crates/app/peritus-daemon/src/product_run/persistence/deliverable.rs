//! Durable candidate projection, independent from interactive mode.
use super::{PersistedDeliverable, ProductRunServiceError};
use peritus_app_protocol::ProductDeliverable;
use peritus_run_settlement::CandidateStage;

impl PersistedDeliverable {
    pub(super) fn from_deliverable(value: &ProductDeliverable) -> Self {
        Self {
            workspace_path: value.workspace_path().to_owned(),
            changed_paths: value.changed_paths().to_vec(),
            successful_commands: value.successful_commands().to_vec(),
            run_instructions: value.run_instructions().to_owned(),
            qualification: Some(value.qualification().tag()),
            accepted: value.accepted(),
            commit_revision: value.commit_revision().to_owned(),
            export_path: value.export_path().to_owned(),
            discarded: value.discarded(),
        }
    }

    pub(super) fn into_deliverable(self) -> Result<ProductDeliverable, ProductRunServiceError> {
        let qualification = self
            .qualification
            .map_or(Some(CandidateStage::Qualified), CandidateStage::from_tag)
            .ok_or(ProductRunServiceError::InvalidMessage)?;
        let mut value = ProductDeliverable::candidate(
            self.workspace_path,
            self.changed_paths,
            self.successful_commands,
            self.run_instructions,
            qualification,
        )
        .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        if self.accepted {
            value = value.mark_accepted();
        }
        if !self.commit_revision.is_empty() {
            value = value
                .mark_committed(self.commit_revision)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        }
        if !self.export_path.is_empty() {
            value = value
                .mark_exported(self.export_path)
                .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        }
        if self.discarded {
            value = value.mark_discarded();
        }
        Ok(value)
    }
}
