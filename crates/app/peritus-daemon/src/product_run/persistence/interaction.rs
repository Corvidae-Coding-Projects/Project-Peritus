//! Strictly checked durable conversation-mode extension.

use super::super::{ProductRunServiceError, interaction::InteractionOptions};
use peritus_app_protocol::{
    MAX_PRODUCT_ACTIVITIES, ProductActivity, ProductActivityKind, ProductInteractionMode,
    ProductModelChoice, ProductRoleModels,
};
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedInteraction {
    mode: u16,
    models: [(String, bool); 3],
    incorporated: u64,
    next_sequence: u64,
    activities: Vec<(u64, u16, String, String)>,
}
impl PersistedInteraction {
    pub(super) fn capture(value: &InteractionOptions) -> Self {
        Self {
            mode: value.mode.tag(),
            models: [value.models.writer(), value.models.reviewer(), value.models.fixer()]
                .map(|choice| (choice.id().to_owned(), choice.manual())),
            incorporated: value.incorporated,
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
        let [writer, reviewer, fixer] = self.models.map(|(id, manual)| {
            if id.is_empty() && !manual {
                Ok(ProductModelChoice::default())
            } else {
                ProductModelChoice::new(id, manual).map_err(|_| invalid())
            }
        });
        let mut value =
            InteractionOptions::new(mode, ProductRoleModels::new(writer?, reviewer?, fixer?));
        if self.activities.len() > MAX_PRODUCT_ACTIVITIES || self.next_sequence == 0 {
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
        if value.activities.windows(2).any(|pair| pair[0].sequence() >= pair[1].sequence())
            || value.activities.last().is_some_and(|last| last.sequence() >= self.next_sequence)
        {
            return Err(invalid());
        }
        value.next_sequence = self.next_sequence;
        value.incorporated = self.incorporated;
        Ok(value)
    }
}
