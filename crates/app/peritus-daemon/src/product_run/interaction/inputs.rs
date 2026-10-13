//! Durable public projection of accepted ledger input, separate from inference admission.

use super::{InteractionOptions, ProductActivityKind, ProductRunService, ProductRunServiceError};
use peritus_product_runner::control::{ControlError, ConversationRecord};
use peritus_types::RunId;

impl ProductRunService {
    pub(in crate::product_run) fn append_control_inputs(
        &self,
        options: &mut InteractionOptions,
    ) -> Result<bool, ProductRunServiceError> {
        let start = &options.workbench;
        let record = self
            .with_controls(false, |store| store.load(start.conversation()))?
            .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
        append_revisions(options, &record)
    }

    pub(super) fn synchronize_public_inputs(
        &self,
        run: RunId,
    ) -> Result<(), ProductRunServiceError> {
        let conversation = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            records
                .get(&run)
                .ok_or(ProductRunServiceError::NotFound)?
                .interaction
                .workbench
                .conversation()
        };
        // Control replay can read substantial immutable history. Do not hold the run registry
        // while waiting for its owner or performing that I/O.
        let inputs = self
            .with_controls(false, |store| store.load(conversation))?
            .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
        if record.interaction.workbench.conversation() != conversation {
            return Err(ProductRunServiceError::Unavailable);
        }
        // A concurrent synchronization may already have published newer input.
        if record.interaction.public_input_count >= inputs.inputs().revisions().len() {
            return Ok(());
        }
        let mut next = record.clone();
        if append_revisions(&mut next.interaction, &inputs)? {
            super::super::persistence::write_record(&self.inner.directory, &next)?;
            *record = next;
        }
        Ok(())
    }
}

fn append_revisions(
    options: &mut InteractionOptions,
    record: &ConversationRecord,
) -> Result<bool, ProductRunServiceError> {
    let revisions = record.inputs().revisions();
    let before = options.public_input_count;
    if options.public_input_count > revisions.len() {
        options.public_input_count = 0;
    }
    for (index, input) in revisions.iter().enumerate().skip(options.public_input_count) {
        options.append(
            ProductActivityKind::User,
            input.text(),
            &format!("Durable input {} received", index + 1),
        )?;
    }
    options.public_input_count = revisions.len();
    Ok(before != options.public_input_count)
}
