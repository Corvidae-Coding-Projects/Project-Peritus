//! The durable reader enforces the same checkpoint invariants as live construction.

use super::{
    CheckpointExclusion, CheckpointId, CheckpointPath, CheckpointReferences, CheckpointText,
    ControlError, UserCheckpoint,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointFields {
    id: CheckpointId,
    name: CheckpointText,
    references: CheckpointReferences,
    paths: Vec<CheckpointPath>,
    exclusions: Vec<CheckpointExclusion>,
    external_effects: Vec<CheckpointText>,
    #[serde(default)]
    automatic_run: Option<[u8; 16]>,
    sealed_by_run: Option<[u8; 16]>,
}

impl TryFrom<CheckpointFields> for UserCheckpoint {
    type Error = ControlError;

    fn try_from(fields: CheckpointFields) -> Result<Self, Self::Error> {
        let value = Self {
            id: fields.id,
            name: fields.name,
            references: fields.references,
            paths: fields.paths,
            exclusions: fields.exclusions,
            external_effects: fields.external_effects,
            automatic_run: fields.automatic_run,
            sealed_by_run: fields.sealed_by_run,
        };
        value.validate()?;
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for UserCheckpoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(CheckpointFields::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
