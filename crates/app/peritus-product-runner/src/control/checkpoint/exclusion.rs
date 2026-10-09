//! Legacy and path-bearing checkpoint exclusion values with stable JSON forms.

use super::{ControlError, ControlText};
use peritus_patch::WorkspacePath;
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserializer, Serializer};

/// Checkpoint exclusion text that can retain a full path and its reason separately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckpointExclusion {
    /// Legacy text retained byte-for-byte for stored checkpoints and short new exclusions.
    Legacy(ControlText<512>),
    /// A path-bearing exclusion whose rendered text exceeds the legacy field size.
    PathReason {
        /// Canonical workspace-relative path.
        path: ControlText<4096>,
        /// Bounded reason the path is excluded from restoration.
        reason: ControlText<512>,
    },
}

impl CheckpointExclusion {
    /// Creates an exclusion from a label and reason, retaining the legacy string form when it
    /// fits and otherwise storing a canonical path and reason as separate fields.
    ///
    /// # Errors
    /// Rejects malformed paths or text beyond the individual field capacities.
    pub fn from_label_and_reason(label: String, reason: String) -> Result<Self, ControlError> {
        let rendered = format!("{label}: {reason}");
        if rendered.len() <= 512 {
            return Ok(Self::Legacy(ControlText::new(rendered)?));
        }
        WorkspacePath::new(&label).map_err(|_| ControlError::InvalidInput)?;
        Ok(Self::PathReason { path: ControlText::new(label)?, reason: ControlText::new(reason)? })
    }

    pub(super) fn rendered(&self) -> String {
        match self {
            Self::Legacy(text) => text.as_str().to_owned(),
            Self::PathReason { path, reason } => format!("{}: {}", path.as_str(), reason.as_str()),
        }
    }
}

impl serde::Serialize for CheckpointExclusion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Legacy(text) => serializer.serialize_str(text.as_str()),
            Self::PathReason { path, reason } => {
                let mut state = serializer.serialize_struct("CheckpointExclusion", 2)?;
                state.serialize_field("path", path.as_str())?;
                state.serialize_field("reason", reason.as_str())?;
                state.end()
            }
        }
    }
}

impl<'de> serde::Deserialize<'de> for CheckpointExclusion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(CheckpointExclusionVisitor)
    }
}

struct CheckpointExclusionVisitor;

impl<'de> Visitor<'de> for CheckpointExclusionVisitor {
    type Value = CheckpointExclusion;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("legacy exclusion text or a path/reason object")
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
        ControlText::<512>::new(value).map(CheckpointExclusion::Legacy).map_err(E::custom)
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut path = None;
        let mut reason = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "path" => {
                    if path.is_some() {
                        return Err(serde::de::Error::duplicate_field("path"));
                    }
                    path = Some(map.next_value::<ControlText<4096>>()?);
                }
                "reason" => {
                    if reason.is_some() {
                        return Err(serde::de::Error::duplicate_field("reason"));
                    }
                    reason = Some(map.next_value::<ControlText<512>>()?);
                }
                _ => return Err(serde::de::Error::unknown_field(&key, &["path", "reason"])),
            }
        }
        let path = path.ok_or_else(|| serde::de::Error::missing_field("path"))?;
        let reason = reason.ok_or_else(|| serde::de::Error::missing_field("reason"))?;
        Ok(CheckpointExclusion::PathReason { path, reason })
    }
}
