//! Durable model selection, independent of user-input admission and task ownership.

use super::{InteractionOptions, ProductRunService, ProductRunServiceError};
use peritus_app_protocol::{ProductActivityKind, ProductInteractionQuery, ProductModelUpdate};

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
            let options = &record.interaction;
            self.authorize_model_selection(actor, options)?;
            (record.request.providers(), options.mode)
        };
        let mut selection = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            records
                .get(&update.run_id())
                .ok_or(ProductRunServiceError::NotFound)?
                .interaction
                .clone()
        };
        selection.models = update.models().clone();
        self.validate_models(providers, &selection.models).await?;
        // Resolve all adapters before mutating durable state. Discovery alone is not support.
        self.resolve_selected_providers(providers, &selection)?;
        {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let record =
                records.get_mut(&update.run_id()).ok_or(ProductRunServiceError::NotFound)?;
            let prior = record.interaction.clone();
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
            record.interaction = next;
            if let Err(error) = super::super::persist_record(&self.inner.directory, record) {
                record.interaction = prior;
                return Err(error);
            }
        }
        self.query_interaction(ProductInteractionQuery::new(update.run_id()))
    }

    fn authorize_model_selection(
        &self,
        actor: peritus_types::ActorId,
        options: &InteractionOptions,
    ) -> Result<(), ProductRunServiceError> {
        let binding = &options.workbench;
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
