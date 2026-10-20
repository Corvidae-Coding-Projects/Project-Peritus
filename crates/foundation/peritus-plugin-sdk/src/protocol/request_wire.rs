//! Manual request-side encoding for the version-one plugin protocol.

use serde::Deserialize;
use serde::de;
use serde::ser::SerializeStruct;
use serde_json::Value;

use super::wire::{
    decode_content, object_value, serialize_tagged, tagged_parts, to_value, unit_variant,
};
use super::{HostRequest, InvocationContext, PluginRequestEnvelope, PluginRole};
use crate::{JsonPayload, PluginId, PluginQuotas, PluginVersion, RequestId};

impl serde::Serialize for PluginRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Plugin => "plugin",
        })
    }
}

impl serde::Serialize for InvocationContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("InvocationContext", 6)?;
        state.serialize_field("session_id", &self.session_id)?;
        state.serialize_field("actor_id", &self.actor_id)?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("granted_capabilities", &self.granted_capabilities)?;
        state.serialize_field("authority_generation", &self.authority_generation)?;
        state.serialize_field("deadline_millis", &self.deadline_millis)?;
        state.end()
    }
}

impl serde::Serialize for HostRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let (method, params) = match self {
            Self::Initialize { protocol_version, plugin_id, plugin_version, quotas } => (
                "initialize",
                Some(object_value([
                    ("protocol_version", to_value::<S::Error, _>(protocol_version)?),
                    ("plugin_id", to_value::<S::Error, _>(plugin_id)?),
                    ("plugin_version", to_value::<S::Error, _>(plugin_version)?),
                    ("quotas", to_value::<S::Error, _>(quotas)?),
                ])),
            ),
            Self::Invoke { capability, input, context } => (
                "invoke",
                Some(object_value([
                    ("capability", to_value::<S::Error, _>(capability)?),
                    ("input", to_value::<S::Error, _>(input)?),
                    ("context", to_value::<S::Error, _>(context)?),
                ])),
            ),
            Self::Cancel { request_id, reason } => (
                "cancel",
                Some(object_value([
                    ("request_id", to_value::<S::Error, _>(request_id)?),
                    ("reason", to_value::<S::Error, _>(reason)?),
                ])),
            ),
            Self::Health => ("health", None),
            Self::Shutdown => ("shutdown", None),
        };
        serialize_tagged(serializer, "method", method, "params", params)
    }
}

impl<'de> Deserialize<'de> for HostRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let (method, params) =
            tagged_parts(value, "method", "params").map_err(de::Error::custom)?;
        match method.as_str() {
            "initialize" => {
                let body: InitializeBody = decode_content(params, "params")?;
                Ok(Self::Initialize {
                    protocol_version: body.protocol_version,
                    plugin_id: body.plugin_id,
                    plugin_version: body.plugin_version,
                    quotas: body.quotas,
                })
            }
            "invoke" => {
                let body: InvokeBody = decode_content(params, "params")?;
                Ok(Self::Invoke {
                    capability: body.capability,
                    input: body.input,
                    context: body.context,
                })
            }
            "cancel" => {
                let body: CancelBody = decode_content(params, "params")?;
                Ok(Self::Cancel { request_id: body.request_id, reason: body.reason })
            }
            "health" => {
                unit_variant::<D::Error>(params.as_ref(), "health")?;
                Ok(Self::Health)
            }
            "shutdown" => {
                unit_variant::<D::Error>(params.as_ref(), "shutdown")?;
                Ok(Self::Shutdown)
            }
            _ => Err(de::Error::unknown_variant(
                &method,
                &["initialize", "invoke", "cancel", "health", "shutdown"],
            )),
        }
    }
}

impl serde::Serialize for PluginRequestEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("PluginRequestEnvelope", 3)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("request_id", &self.request_id)?;
        state.serialize_field("request", &self.request)?;
        state.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializeBody {
    protocol_version: u16,
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    quotas: PluginQuotas,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeBody {
    capability: String,
    input: JsonPayload,
    context: InvocationContext,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelBody {
    request_id: RequestId,
    reason: String,
}
