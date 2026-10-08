//! Response encoding without intermediate JSON value trees.

use serde::{
    Deserialize,
    de,
    ser::{SerializeMap, SerializeStruct},
};
use serde_json::value::RawValue;

use super::{FailureClass, PluginFailure, PluginResponse, PluginResponseEnvelope, PluginStatus};
use crate::{JsonPayload, RequestId};

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
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            Self::Status { status } => {
                map.serialize_entry("kind", "status")?;
                map.serialize_entry("body", &StatusRef { status: *status })?;
            }
            Self::Success { output, rendering } => {
                map.serialize_entry("kind", "success")?;
                map.serialize_entry("body", &SuccessRef { output, rendering })?;
            }
            Self::Failure(failure) => {
                map.serialize_entry("kind", "failure")?;
                map.serialize_entry("body", failure)?;
            }
        }
        map.end()
    }
}

struct StatusRef {
    status: PluginStatus,
}

impl serde::Serialize for StatusRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("StatusBody", 1)?;
        state.serialize_field("status", &self.status)?;
        state.end()
    }
}

struct SuccessRef<'a> {
    output: &'a JsonPayload,
    rendering: &'a Option<String>,
}

impl serde::Serialize for SuccessRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("SuccessBody", 2)?;
        state.serialize_field("output", self.output)?;
        state.serialize_field("rendering", self.rendering)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for PluginResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        decode_response(&raw)
    }
}

fn decode_response<E>(raw: &RawValue) -> Result<PluginResponse, E>
where
    E: de::Error,
{
    let wire: TaggedResponse<'_> = decode_raw(raw, "plugin response")?;
    match wire.kind.as_str() {
        "status" => {
            let body: StatusBody = decode_raw(wire.body, "status body")?;
            Ok(PluginResponse::Status { status: body.status })
        }
        "success" => {
            let body: SuccessBody<'_> = decode_raw(wire.body, "success body")?;
            let output = JsonPayload::parse_active(body.output.get().as_bytes())
                .map_err(E::custom)?;
            let rendering = body
                .rendering
                .map(|raw| decode_raw::<String, E>(raw, "success rendering"))
                .transpose()?;
            Ok(PluginResponse::Success {
                output,
                rendering,
            })
        }
        "failure" => Ok(PluginResponse::Failure(decode_raw(wire.body, "failure body")?)),
        _ => Err(E::unknown_variant(&wire.kind, &["status", "success", "failure"])),
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

impl<'de> Deserialize<'de> for PluginResponseEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ResponseEnvelopeWire::deserialize(deserializer)?;
        Ok(Self {
            protocol_version: wire.protocol_version,
            request_id: wire.request_id,
            response: decode_response(wire.response)?,
        })
    }
}

fn decode_raw<'a, T, E>(raw: &'a RawValue, label: &'static str) -> Result<T, E>
where
    T: Deserialize<'a>,
    E: de::Error,
{
    serde_json::from_str(raw.get())
        .map_err(|error| E::custom(format!("malformed {label}: {error}")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResponseEnvelopeWire<'a> {
    protocol_version: u16,
    request_id: RequestId,
    #[serde(borrow)]
    response: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaggedResponse<'a> {
    kind: String,
    #[serde(borrow)]
    body: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusBody {
    status: PluginStatus,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuccessBody<'a> {
    #[serde(borrow)]
    output: &'a RawValue,
    #[serde(borrow)]
    rendering: Option<&'a RawValue>,
}
