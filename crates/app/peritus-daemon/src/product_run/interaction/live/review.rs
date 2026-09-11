//! Read-only review-state access shared by both compilation configurations.

use super::{LiveConversation, ProductRunServiceError};

impl LiveConversation {
    pub(super) fn workbench_start_record(
        &self,
    ) -> Result<Option<peritus_product_runner::control::ControlOperation>, ProductRunServiceError>
    {
        let records =
            self.service.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&self.run_id).ok_or(ProductRunServiceError::NotFound)?;
        let options = record.interaction.as_ref().ok_or(ProductRunServiceError::NotFound)?;
        if options.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ProductRunServiceError::Unavailable);
        }
        Ok(options.workbench.clone())
    }

    pub(super) fn review_record(
        &self,
    ) -> Result<Option<peritus_product_runner::control::ConversationRecord>, ProductRunServiceError>
    {
        let records =
            self.service.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&self.run_id).ok_or(ProductRunServiceError::NotFound)?;
        let Some(start) =
            record.interaction.as_ref().and_then(|options| options.workbench.as_ref())
        else {
            return Ok(None);
        };
        self.service
            .with_controls(false, |store| {
                store
                    .load(start.conversation())?
                    .map(Some)
                    .ok_or_else(|| peritus_product_runner::control::ControlError::NotFound.into())
            })
            .map_err(Into::into)
    }
}
