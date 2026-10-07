//! Durable public projection of accepted ledger input, separate from inference admission.

use super::{InteractionOptions, ProductActivityKind, ProductRunService, ProductRunServiceError};
use crate::product_run::publication::{MutationDisposition, RunMutationKind};
use peritus_product_runner::control::ControlError;
use peritus_types::RunId;

impl ProductRunService {
    pub(in crate::product_run) fn append_control_inputs(
        &self,
        options: &mut InteractionOptions,
    ) -> Result<bool, ProductRunServiceError> {
        let inputs = self.control_input_texts(&options.workbench)?;
        Self::append_control_input_texts(options, &inputs)
    }

    pub(in crate::product_run) fn control_input_texts(
        &self,
        start: &peritus_product_runner::control::ControlOperation,
    ) -> Result<Vec<String>, ProductRunServiceError> {
        let record = self
            .with_control_conversation(start.conversation(), |store| {
                store.load(start.conversation())
            })?
            .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
        Ok(record
            .inputs()
            .revisions()
            .iter()
            .map(|input| input.text().to_owned())
            .collect())
    }

    pub(in crate::product_run) fn append_control_input_texts(
        options: &mut InteractionOptions,
        inputs: &[String],
    ) -> Result<bool, ProductRunServiceError> {
        if options.public_input_count > inputs.len() {
            options.public_input_count = 0;
        }
        let before = options.public_input_count;
        for (index, input) in inputs.iter().enumerate().skip(options.public_input_count) {
            options.append(
                ProductActivityKind::User,
                input,
                &format!("Durable input {} received", index + 1),
            )?;
        }
        options.public_input_count = inputs.len();
        Ok(before != options.public_input_count)
    }

    pub(super) fn synchronize_public_inputs(
        &self,
        run: RunId,
    ) -> Result<(), ProductRunServiceError> {
        let identity = self.capture_run_identity(run)?;
        let inputs = self.control_input_texts(&identity.start)?;
        if identity.interaction.public_input_count == inputs.len() {
            return Ok(());
        }
        let mut input = b"peritus-product-run-interaction-input-v1\0".to_vec();
        input.extend_from_slice(run.as_bytes());
        for text in &inputs {
            input.extend_from_slice(&(text.len() as u64).to_be_bytes());
            input.extend_from_slice(text.as_bytes());
        }
        let expected_attempt = std::sync::Arc::clone(&identity.cancelled);
        let (_, ticket) = self.mutate_run(
            run,
            Some(&expected_attempt),
            RunMutationKind::InteractionInput,
            peritus_codec::sha256(&input),
            MutationDisposition::DurabilityRequired,
            move |record| {
                if record.interaction.workbench != identity.start {
                    return Err(ProductRunServiceError::InvalidState);
                }
                Self::append_control_input_texts(&mut record.interaction, &inputs)?;
                Ok(())
            },
        )?;
        self.await_run_durable(ticket)
    }
}
