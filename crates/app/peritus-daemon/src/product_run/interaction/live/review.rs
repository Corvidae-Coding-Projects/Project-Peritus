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
        let options = &record.interaction;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ProductRunServiceError::Unavailable);
        }
        Ok(options.workbench.clone())
    }

    pub(super) fn review_record(
        &self,
    ) -> Result<peritus_product_runner::control::ConversationRecord, ProductRunServiceError> {
        let record = self.attempt_record()?;
        let start = record.interaction.workbench;
        self.service
            .with_control_conversation(start.conversation(), |store| {
                store
                    .load(start.conversation())?
                    .ok_or_else(|| peritus_product_runner::control::ControlError::NotFound.into())
            })
            .map_err(Into::into)
    }
}
