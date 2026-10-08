//! Exclusion reasons retain source identity separately from their inert display explanation.

use super::{CheckpointText, ControlError};
use serde::{Deserialize, Serialize};

/// Closed host observation explaining why a source has no checkpoint coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointExclusionReason {
    /// The user deselected this source.
    Deselected,
    /// An external import has no workspace target.
    ExternalImport,
    /// The source belongs to a different observed workspace folder.
    FolderMismatch,
}

impl CheckpointExclusionReason {
    const fn explanation(self) -> &'static str {
        match self {
            Self::Deselected => "deselected",
            Self::ExternalImport => "external import has no workspace target",
            Self::FolderMismatch => "folder identity differs from the current workspace",
        }
    }
}

/// Exact historical string or a checked structured exclusion. Neither grants path authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointExclusion {
    /// Preserves an already accepted string and its canonical bytes exactly.
    Legacy(CheckpointText),
    /// Separately binds the source label, optional native path, reason, and explanation.
    Structured(CheckpointExclusionDetails),
}

/// Concrete borrowed iterator over the full visible exclusion explanations.
pub type CheckpointExclusions<'a> = std::iter::Map<
    std::slice::Iter<'a, CheckpointExclusion>,
    fn(&'a CheckpointExclusion) -> &'a str,
>;

impl Serialize for CheckpointExclusion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Legacy(text) => text.serialize(serializer),
            Self::Structured(details) => details.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for CheckpointExclusion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ExclusionVisitor;
        impl<'de> serde::de::Visitor<'de> for ExclusionVisitor {
            type Value = CheckpointExclusion;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("legacy exclusion text or checked exclusion fields")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                CheckpointExclusion::legacy(value.to_owned()).map_err(E::custom)
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                CheckpointExclusion::legacy(value).map_err(E::custom)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                CheckpointExclusionDetails::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
                .map(CheckpointExclusion::Structured)
            }
        }
        deserializer.deserialize_any(ExclusionVisitor)
    }
}

impl CheckpointExclusion {
    /// Constructs a structured host observation without concatenating into a bounded field.
    ///
    /// # Errors
    /// Rejects malformed inert metadata. An excluded source path does not grant effect authority.
    pub fn new(
        label: String,
        path: Option<String>,
        reason: CheckpointExclusionReason,
    ) -> Result<Self, ControlError> {
        Ok(Self::Structured(CheckpointExclusionDetails::from_fields(ExclusionFields {
            label: CheckpointText::new(label)?,
            path: path.map(CheckpointText::new).transpose()?,
            reason,
            explanation: CheckpointText::new(reason.explanation().to_owned())?,
        })?))
    }

    pub(super) fn legacy(text: String) -> Result<Self, ControlError> {
        Ok(Self::Legacy(CheckpointText::new(text)?))
    }

    /// Borrows the complete inert display value, including its source label.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Legacy(text) => text.as_str(),
            Self::Structured(details) => details.display.as_str(),
        }
    }

    /// Borrows the separately retained reason and source fields when available.
    #[must_use]
    pub const fn details(&self) -> Option<&CheckpointExclusionDetails> {
        match self {
            Self::Legacy(_) => None,
            Self::Structured(details) => Some(details),
        }
    }
}

/// Checked reason/source fields with a rebuilt display cache, never an effect capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckpointExclusionDetails {
    fields: ExclusionFields,
    display: CheckpointText,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExclusionFields {
    label: CheckpointText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<CheckpointText>,
    reason: CheckpointExclusionReason,
    explanation: CheckpointText,
}

impl CheckpointExclusionDetails {
    fn from_fields(fields: ExclusionFields) -> Result<Self, ControlError> {
        let display = CheckpointText::new(format!(
            "{}: {}",
            fields.label.as_str(),
            fields.explanation.as_str()
        ))?;
        Ok(Self { fields, display })
    }

    /// Borrows the exact source label independently of the explanation.
    #[must_use]
    pub fn label(&self) -> &str {
        self.fields.label.as_str()
    }

    /// Borrows an observed native workspace target, when the source had one.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.fields.path.as_ref().map(CheckpointText::as_str)
    }

    /// Returns the closed exclusion reason.
    #[must_use]
    pub const fn reason(&self) -> CheckpointExclusionReason {
        self.fields.reason
    }

    /// Borrows the retained inert explanation independently of the path and source label.
    #[must_use]
    pub fn explanation(&self) -> &str {
        self.fields.explanation.as_str()
    }
}

impl Serialize for CheckpointExclusionDetails {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.fields.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CheckpointExclusionDetails {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_fields(ExclusionFields::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
