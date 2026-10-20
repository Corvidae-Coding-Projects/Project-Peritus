//! Manual response-side encoding for the version-one plugin protocol.

use serde::Deserialize;
use serde::de;
use serde::ser::SerializeStruct;
use serde_json::Value;

use super::wire::{decode_content, object_value, serialize_tagged, tagged_parts, to_value};
use super::{FailureClass, PluginFailure, PluginResponse, PluginResponseEnvelope, PluginStatus};
use crate::JsonPayload;

impl serde::Serialize for FailureClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Protocol => "protocol",
            Self::Authorization => "authorization",
            Self::Unsupported => "unsupported",
            Self::InvalidInput => "invalid-input",
            Self::Quota => "quota",
            Self::Plugin => "plugin",
            Self::Infrastructure => "infrastructure",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::Indeterminate => "indeterminate",
        })
    }
}

impl<'de> Deserialize<'de> for FailureClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "protocol" => Ok(Self::Protocol),
            "authorization" => Ok(Self::Authorization),
            "unsupported" => Ok(Self::Unsupported),
            "invalid-input" => Ok(Self::InvalidInput),
            "quota" => Ok(Self::Quota),
            "plugin" => Ok(Self::Plugin),
            "infrastructure" => Ok(Self::Infrastructure),
            "cancelled" => Ok(Self::Cancelled),
            "timeout" => Ok(Self::Timeout),
            "indeterminate" => Ok(Self::Indeterminate),
            _ => Err(de::Error::unknown_variant(
                &value,
                &[
                    "protocol",
                    "authorization",
                    "unsupported",
                    "invalid-input",
                    "quota",
                    "plugin",
                    "infrastructure",
                    "cancelled",
                    "timeout",
                    "indeterminate",
                ],
            )),
        }
    }
}

impl serde::Serialize for PluginStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Ready => "ready",
            Self::Healthy => "healthy",
            Self::Cancelled => "cancelled",
            Self::Stopped => "stopped",
        })
    }
}

impl serde::Serialize for PluginFailure {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PluginFailure", 4)?;
        state.serialize_field("class", &self.class)?;
        state.serialize_field("code", &self.code)?;
        state.serialize_field("detail", &self.detail)?;
        state.serialize_field("retryable_with_new_action", &self.retryable_with_new_action)?;
        state.end()
    }
}

impl serde::Serialize for PluginResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (kind, body) = match self {
            Self::Status { status } => {
                ("status", object_value([("status", to_value::<S::Error, _>(status)?)]))
            }
            Self::Success { output, rendering } => (
                "success",
                object_value([
                    ("output", to_value::<S::Error, _>(output)?),
                    ("rendering", to_value::<S::Error, _>(rendering)?),
                ]),
            ),
            Self::Failure(failure) => ("failure", to_value::<S::Error, _>(failure)?),
        };
        serialize_tagged(serializer, "kind", kind, "body", Some(body))
    }
}

impl<'de> Deserialize<'de> for PluginResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let (kind, body) = tagged_parts(value, "kind", "body").map_err(de::Error::custom)?;
        match kind.as_str() {
            "status" => {
                let body: StatusBody = decode_content(body, "body")?;
                Ok(Self::Status { status: body.status })
            }
            "success" => {
                let body: SuccessBody = decode_content(body, "body")?;
                Ok(Self::Success { output: body.output, rendering: body.rendering })
            }
            "failure" => Ok(Self::Failure(decode_content(body, "body")?)),
            _ => Err(de::Error::unknown_variant(&kind, &["status", "success", "failure"])),
        }
    }
}

impl serde::Serialize for PluginResponseEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PluginResponseEnvelope", 3)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("request_id", &self.request_id)?;
        state.serialize_field("response", &self.response)?;
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusBody {
    status: PluginStatus,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuccessBody {
    output: JsonPayload,
    rendering: Option<String>,
}
