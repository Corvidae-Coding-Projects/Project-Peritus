//! Durable model selection, independent of user-input admission and task ownership.

use super::{InteractionOptions, ProductRunService, ProductRunServiceError};
use peritus_app_protocol::{ProductActivityKind, ProductModelUpdate, ProductRunConversationQuery};

impl ProductRunService {
    pub(crate) async fn update_models(
        &self,
        actor: peritus_types::ActorId,
        update: &ProductModelUpdate,
    ) -> Result<peritus_app_protocol::ProductInteractionSnapshot, ProductRunServiceError> {
        let (providers, mode) = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record = records.get(&update.run_id()).ok_or(ProductRunServiceError::NotFound)?;
            let options =
                record.interaction.as_ref().ok_or(ProductRunServiceError::InvalidState)?;
            self.authorize_model_selection(actor, options)?;
            (record.request.providers(), options.mode)
        };
        let selection = InteractionOptions::new(mode, update.models().clone());
        self.validate_models(providers, &selection).await?;
        // Resolve all adapters before mutating durable state. Discovery alone is not support.
        self.resolve_selected_providers(providers, Some(&selection))?;
        {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record =
                records.get_mut(&update.run_id()).ok_or(ProductRunServiceError::NotFound)?;
            let prior =
                record.interaction.as_ref().ok_or(ProductRunServiceError::InvalidState)?.clone();
            self.authorize_model_selection(actor, &prior)?;
            if prior.mode != mode {
                return Err(ProductRunServiceError::InvalidState);
            }
            if prior.persistence_failed.load(std::sync::atomic::Ordering::Acquire) {
                return Err(ProductRunServiceError::Unavailable);
            }
            let mut next = prior.clone();
            next.models = update.models().clone();
            next.append(ProductActivityKind::Status,
                "Model selection saved for subsequent model turns; any in-flight turn is unchanged.", "")?;
            record.interaction = Some(next);
            if let Err(error) = super::super::persist_record(&self.inner.directory, record) {
                record.interaction = Some(prior);
                return Err(error);
            }
        }
        self.query_interaction(ProductRunConversationQuery::new(update.run_id()))
    }

    fn authorize_model_selection(
        &self,
        actor: peritus_types::ActorId,
        options: &InteractionOptions,
    ) -> Result<(), ProductRunServiceError> {
        let Some(binding) = &options.workbench else { return Ok(()) };
        self.with_controls(false, |store| {
            let record = store
                .load(binding.conversation())?
                .ok_or(peritus_product_runner::control::ControlError::NotFound)?;
            if binding.actor_bytes() != actor.as_bytes()
                || record.owner_bytes() != actor.as_bytes()
                || record.workspace_bytes() != binding.workspace_bytes()
            {
                return Err(peritus_product_runner::control::ControlError::ScopeMismatch.into());
            }
            Ok(())
        })
        .map_err(Into::into)
    }
}
