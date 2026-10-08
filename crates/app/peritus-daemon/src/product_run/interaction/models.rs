//! Durable model selection, independent of user-input admission and task ownership.

use super::{InteractionOptions, ProductRunService, ProductRunServiceError};
use crate::product_run::publication::{MutationDisposition, RunMutationKind};
use peritus_app_protocol::{ProductActivityKind, ProductInteractionQuery, ProductModelUpdate};

impl ProductRunService {
    pub(crate) async fn update_models(
        &self,
        actor: peritus_types::ActorId,
        update: &ProductModelUpdate,
    ) -> Result<peritus_app_protocol::ProductInteractionSnapshot, ProductRunServiceError> {
        let service = self.clone();
        let update = update.clone();
        Self::await_blocking_future("update durable product-run models", move || async move {
            service.update_models_owned(actor, update).await
        })
        .await?
    }

    async fn update_models_owned(
        &self,
        actor: peritus_types::ActorId,
        update: ProductModelUpdate,
    ) -> Result<peritus_app_protocol::ProductInteractionSnapshot, ProductRunServiceError> {
        let run = update.run_id();
        let identity = self.capture_run_identity(run)?;
        self.authorize_model_selection(actor, &identity.interaction)?;
        let providers = identity.request.providers();
        let mode = identity.interaction.mode;
        let requested_models = update.models().clone();
        let mut selection = identity.interaction.clone();
        selection.models = requested_models.clone();
        self.validate_models(providers, &selection.models).await?;
        // Discovery establishes the exact model identity, not its complete execution contract.
        // Retain such a selection under this run so provider setup can resolve its facts without
        // substituting a model or losing the user's explicit choice.
        let pending_facts = self.validate_model_resolution(providers, &selection.models)?;
        let mut input = b"peritus-product-run-model-selection-v1\0".to_vec();
        input.extend_from_slice(run.as_bytes());
        input.extend_from_slice(actor.as_bytes());
        for choice in [
            requested_models.writer(),
            requested_models.reviewer(),
            requested_models.fixer(),
        ] {
            input.extend_from_slice(&(choice.id().len() as u64).to_be_bytes());
            input.extend_from_slice(choice.id().as_bytes());
            input.push(u8::from(choice.manual()));
            input.extend_from_slice(&choice.effort().tag().to_be_bytes());
        }
        let expected_attempt = std::sync::Arc::clone(&identity.cancelled);
        let (_, ticket) = self.mutate_run(
            run,
            Some(&expected_attempt),
            RunMutationKind::ModelSelection,
            peritus_codec::sha256(&input),
            MutationDisposition::DurabilityRequired,
            move |record| {
                let prior = record.interaction.clone();
                if record.request.workspace_id() != identity.workspace
                    || prior.workbench != identity.start
                    || prior.mode != mode
                    || prior.models != identity.interaction.models
                    || prior.workbench.actor_bytes() != actor.as_bytes()
                {
                    return Err(ProductRunServiceError::InvalidState);
                }
                let mut next = prior.clone();
                next.models = requested_models;
                let status = if pending_facts {
                    "Model selection saved pending exact capacity and feature facts. Open provider setup, select the same model on this provider route, and finish setup before its next turn. Any in-flight turn is unchanged."
                } else {
                    "Model selection saved for subsequent model turns; any in-flight turn is unchanged."
                };
                next.append(
                    ProductActivityKind::Status,
                    status,
                    "",
                )?;
                record.interaction = next;
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)?;
        self.query_interaction(ProductInteractionQuery::new(run))
    }

    fn authorize_model_selection(
        &self,
        actor: peritus_types::ActorId,
        options: &InteractionOptions,
    ) -> Result<(), ProductRunServiceError> {
        let binding = &options.workbench;
        self.with_control_conversation(binding.conversation(), |store| {
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
