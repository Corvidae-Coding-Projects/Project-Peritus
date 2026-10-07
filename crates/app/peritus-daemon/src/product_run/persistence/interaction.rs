//! Strictly checked durable conversation-mode extension.

use super::super::{ProductRunServiceError, interaction::InteractionOptions};
use peritus_app_protocol::{
    ProductActivity, ProductActivityKind, ProductInteractionMode, ProductModelChoice,
    ProductModelEffort, ProductRoleModels,
};
use serde::Deserialize;
use serde::Serialize;
use serde::ser::SerializeStruct;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedInteraction {
    pub(super) workbench: peritus_product_runner::control::ControlOperation,
    mode: u16,
    models: [(String, bool); 3],
    efforts: [u16; 3],
    incorporated: u64,
    public_input_count: usize,
    next_sequence: u64,
    activities: Vec<(u64, u16, String, String)>,
}
impl PersistedInteraction {
    pub(super) fn capture(value: &InteractionOptions) -> Self {
        Self {
            workbench: value.workbench.clone(),
            mode: value.mode.tag(),
            models: [value.models.writer(), value.models.reviewer(), value.models.fixer()]
                .map(|choice| (choice.id().to_owned(), choice.manual())),
            efforts: [value.models.writer(), value.models.reviewer(), value.models.fixer()]
                .map(|choice| choice.effort().tag()),
            incorporated: value.incorporated,
            public_input_count: value.public_input_count,
            next_sequence: value.next_sequence,
            activities: value
                .activities
                .iter()
                .map(|activity| {
                    (
                        activity.sequence(),
                        activity.kind().tag(),
                        activity.text().to_owned(),
                        activity.detail().to_owned(),
                    )
                })
                .collect(),
        }
    }
    pub(super) fn restore(self) -> Result<InteractionOptions, ProductRunServiceError> {
        let invalid = || ProductRunServiceError::InvalidMessage;
        let mode = ProductInteractionMode::from_tag(self.mode).ok_or_else(invalid)?;
        let [writer, reviewer, fixer] = [0, 1, 2].map(|index| {
            let (id, manual) = &self.models[index];
            let choice = if id.is_empty() && !manual {
                Ok(ProductModelChoice::default())
            } else {
                ProductModelChoice::new(id.clone(), *manual).map_err(|_| invalid())
            }?;
            Ok::<_, ProductRunServiceError>(choice.with_effort(
                ProductModelEffort::from_tag(self.efforts[index]).ok_or_else(invalid)?,
            ))
        });
        self.workbench.canonical_bytes().map_err(|_| invalid())?;
        if !matches!(
            self.workbench.intent(),
            peritus_product_runner::control::ControlIntent::StartExecution { .. }
                | peritus_product_runner::control::ControlIntent::StartGoal { .. }
        ) {
            return Err(invalid());
        }
        let mut value = InteractionOptions::new(
            self.workbench,
            mode,
            ProductRoleModels::new(writer?, reviewer?, fixer?),
        );
        if self.next_sequence == 0 {
            return Err(invalid());
        }
        value.activities = self
            .activities
            .into_iter()
            .map(|(sequence, kind, text, detail)| {
                ProductActivity::new(
                    sequence,
                    ProductActivityKind::from_tag(kind).ok_or_else(invalid)?,
                    text,
                    detail,
                )
                .map_err(|_| invalid())
            })
            .collect::<Result<_, _>>()?;
        if value.activities.windows(2).any(|pair| {
            pair[0].sequence().checked_add(1) != Some(pair[1].sequence())
        })
            || value
                .activities
                .last()
                .is_some_and(|last| last.sequence().checked_add(1) != Some(self.next_sequence))
            || value.activities.is_empty() && self.next_sequence != 1
        {
            return Err(invalid());
        }
        value.next_sequence = self.next_sequence;
        value.incorporated = self.incorporated;
        value.public_input_count = self.public_input_count;
        Ok(value)
    }
}

impl Serialize for PersistedInteraction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("PersistedInteraction", 8)?;
        state.serialize_field("workbench", &self.workbench)?;
        state.serialize_field("mode", &self.mode)?;
        state.serialize_field("models", &self.models)?;
        state.serialize_field("efforts", &self.efforts)?;
        state.serialize_field("public_input_count", &self.public_input_count)?;
        state.serialize_field("incorporated", &self.incorporated)?;
        state.serialize_field("next_sequence", &self.next_sequence)?;
        state.serialize_field("activities", &self.activities)?;
        state.end()
    }
}
