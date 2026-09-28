//! Durable public projection of accepted ledger input, separate from inference admission.

use super::{
    InteractionOptions, ProductActivity, ProductActivityKind, ProductRunService,
    ProductRunServiceError,
};
use peritus_product_runner::control::ControlError;
use peritus_types::RunId;

impl ProductRunService {
    pub(in crate::product_run) fn append_control_inputs(
        &self,
        options: &mut InteractionOptions,
    ) -> Result<bool, ProductRunServiceError> {
        let Some(start) = &options.workbench else {
            return Ok(false);
        };
        let record = self
            .with_controls(false, |store| store.load(start.conversation()))?
            .ok_or(ProductRunServiceError::Control(ControlError::NotFound))?;
        let revisions = record.inputs().revisions();
        if options.public_input_count > revisions.len() {
            options.public_input_count = 0;
        }
        let before = options.public_input_count;
        // Older projections used the inert execution label as the first user message.
        if before == 0
            && let Some(first) = revisions.first()
            && let Some(activity) = options.activities.first_mut().filter(|activity| {
                activity.sequence() == 1
                    && activity.kind() == ProductActivityKind::User
                    && activity.text() == "Execute the selected durable workbench inputs."
            })
        {
            *activity = ProductActivity::new(
                1,
                ProductActivityKind::User,
                first.text().to_owned(),
                "Durable input 1 received".to_owned(),
            )
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
            options.public_input_count = 1;
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

    pub(super) fn synchronize_public_inputs(
        &self,
        run: RunId,
    ) -> Result<(), ProductRunServiceError> {
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run).ok_or(ProductRunServiceError::NotFound)?;
        let mut next = record.clone();
        if let Some(options) = next.interaction.as_mut()
            && self.append_control_inputs(options)?
        {
            super::super::persistence::write_record(&self.inner.directory, &next)?;
            *record = next;
        }
        Ok(())
    }
}
