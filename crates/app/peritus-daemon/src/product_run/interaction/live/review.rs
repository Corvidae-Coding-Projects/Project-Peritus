//! Read-only review-state access shared by both compilation configurations.

use super::{LiveConversation, ProductRunServiceError};

impl LiveConversation {
    pub(super) fn attempt_record(
        &self,
    ) -> Result<crate::product_run::RunRecord, ProductRunServiceError> {
        let record = {
            let records =
                self.service.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            records.get(&self.run_id).cloned().ok_or(ProductRunServiceError::NotFound)?
        };
        if !std::sync::Arc::ptr_eq(&record.cancelled, &self.attempt_cancelled) {
            return Err(ProductRunServiceError::InvalidState);
        }
        Ok(record)
    }

    pub(super) fn workbench_start_record(
        &self,
    ) -> Result<peritus_product_runner::control::ControlOperation, ProductRunServiceError> {
        let record = self.attempt_record()?;
        Ok(record.interaction.workbench)
    }
}
