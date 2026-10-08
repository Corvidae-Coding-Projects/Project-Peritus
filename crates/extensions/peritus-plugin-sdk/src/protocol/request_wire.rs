//! Manual request-side encoding for the version-one plugin protocol.

use serde::Deserialize;
use serde::de;
use serde::ser::{Error as _, SerializeStruct};
use serde_json::Value;

use super::wire::{
    decode_content, object_value, serialize_tagged, tagged_parts, to_value, unit_variant,
};
use super::{
    HostRequest, InvocationContext, LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION,
    PluginRequestEnvelope, PluginRole,
};
use crate::{
    CumulativeQuota, JsonPayload, PluginId, PluginQuotas, PluginVersion, RequestId,
};

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
        let mut state =
            serializer.serialize_struct("InvocationContext", 5 + usize::from(self.deadline_millis.is_some()))?;
        state.serialize_field("session_id", &self.session_id)?;
        state.serialize_field("actor_id", &self.actor_id)?;
        state.serialize_field("role", &self.role)?;
        state.serialize_field("granted_capabilities", &self.granted_capabilities)?;
        state.serialize_field("authority_generation", &self.authority_generation)?;
        if let Some(deadline_millis) = self.deadline_millis {
            state.serialize_field("deadline_millis", &deadline_millis)?;
        }
        state.end()
    }
}

impl serde::Serialize for HostRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_request(self, PROTOCOL_VERSION, serializer)
    }
}

fn serialize_request<S>(
    request: &HostRequest,
    wire_version: u16,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
        if !matches!(wire_version, LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION) {
            return Err(S::Error::custom("unsupported plugin request protocol version"));
        }
        let (method, params) = match request {
            HostRequest::Initialize {
                protocol_version: selected_version,
                plugin_id,
                plugin_version,
                quotas,
            } => (
                "initialize",
                Some(object_value([
                    ("protocol_version", to_value::<S::Error, _>(selected_version)?),
                    ("plugin_id", to_value::<S::Error, _>(plugin_id)?),
                    ("plugin_version", to_value::<S::Error, _>(plugin_version)?),
                    (
                        "quotas",
                        if wire_version == LEGACY_PROTOCOL_VERSION {
                            to_value::<S::Error, _>(&LegacyQuotasRef(quotas))?
                        } else {
                            to_value::<S::Error, _>(quotas)?
                        },
                    ),
                ])),
            ),
            HostRequest::Invoke { capability, input, context } => (
                "invoke",
                Some(object_value([
                    ("capability", to_value::<S::Error, _>(capability)?),
                    ("input", to_value::<S::Error, _>(input)?),
                    (
                        "context",
                        if wire_version == LEGACY_PROTOCOL_VERSION {
                            to_value::<S::Error, _>(&LegacyContextRef(context))?
                        } else {
                            to_value::<S::Error, _>(context)?
                        },
                    ),
                ])),
            ),
            HostRequest::Cancel { request_id, reason } => (
                "cancel",
                Some(object_value([
                    ("request_id", to_value::<S::Error, _>(request_id)?),
                    ("reason", to_value::<S::Error, _>(reason)?),
                ])),
            ),
            HostRequest::Health => ("health", None),
            HostRequest::Shutdown => ("shutdown", None),
        };
        serialize_tagged(serializer, "method", method, "params", params)
}

struct LegacyQuotasRef<'a>(&'a PluginQuotas);

impl serde::Serialize for LegacyQuotasRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let Some((invocation_millis, lifecycle_requests, protocol_violations)) =
            self.0.legacy_values()
        else {
            return Err(S::Error::custom("protocol version one requires finite numeric quotas"));
        };
        let mut state = serializer.serialize_struct("PluginQuotas", 6)?;
        state.serialize_field("concurrent_requests", &self.0.concurrent_requests)?;
        state.serialize_field("frame_bytes", &self.0.frame_bytes)?;
        state.serialize_field("output_bytes", &self.0.output_bytes)?;
        state.serialize_field("invocation_millis", &invocation_millis)?;
        state.serialize_field("lifecycle_requests", &lifecycle_requests)?;
        state.serialize_field("protocol_violations", &protocol_violations)?;
        state.end()
    }
}

struct LegacyContextRef<'a>(&'a InvocationContext);

impl serde::Serialize for LegacyContextRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let deadline_millis = self
            .0
            .deadline_millis
            .ok_or_else(|| S::Error::custom("protocol version one requires an invocation deadline"))?;
        let mut state = serializer.serialize_struct("InvocationContext", 6)?;
        state.serialize_field("session_id", &self.0.session_id)?;
        state.serialize_field("actor_id", &self.0.actor_id)?;
        state.serialize_field("role", &self.0.role)?;
        state.serialize_field("granted_capabilities", &self.0.granted_capabilities)?;
        state.serialize_field("authority_generation", &self.0.authority_generation)?;
        state.serialize_field("deadline_millis", &deadline_millis)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for HostRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        decode_request(value, PROTOCOL_VERSION)
    }
}

fn decode_request<E>(value: Value, protocol_version: u16) -> Result<HostRequest, E>
where
    E: de::Error,
{
        let (method, params) =
            tagged_parts(value, "method", "params").map_err(E::custom)?;
        match method.as_str() {
            "initialize" => {
                if protocol_version == LEGACY_PROTOCOL_VERSION {
                    let body: LegacyInitializeBody = decode_content(params, "params")?;
                    Ok(HostRequest::Initialize {
                        protocol_version: body.protocol_version,
                        plugin_id: body.plugin_id,
                        plugin_version: body.plugin_version,
                        quotas: body.quotas.migrate(),
                    })
                } else {
                    let body: InitializeBody = decode_content(params, "params")?;
                    Ok(HostRequest::Initialize {
                        protocol_version: body.protocol_version,
                        plugin_id: body.plugin_id,
                        plugin_version: body.plugin_version,
                        quotas: body.quotas,
                    })
                }
            }
            "invoke" => {
                if protocol_version == LEGACY_PROTOCOL_VERSION {
                    let body: LegacyInvokeBody = decode_content(params, "params")?;
                    Ok(HostRequest::Invoke {
                        capability: body.capability,
                        input: body.input,
                        context: body.context.migrate(),
                    })
                } else {
                    let body: InvokeBody = decode_content(params, "params")?;
                    Ok(HostRequest::Invoke {
                        capability: body.capability,
                        input: body.input,
                        context: body.context,
                    })
                }
            }
            "cancel" => {
                let body: CancelBody = decode_content(params, "params")?;
                Ok(HostRequest::Cancel { request_id: body.request_id, reason: body.reason })
            }
            "health" => {
                unit_variant::<D::Error>(params.as_ref(), "health")?;
                Ok(HostRequest::Health)
            }
            "shutdown" => {
                unit_variant::<D::Error>(params.as_ref(), "shutdown")?;
                Ok(HostRequest::Shutdown)
            }
            _ => Err(de::Error::unknown_variant(
                &method,
                &["initialize", "invoke", "cancel", "health", "shutdown"],
            )),
        }
}

impl serde::Serialize for PluginRequestEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        validate_request(self.protocol_version, &self.request).map_err(S::Error::custom)?;
        let mut state = serializer.serialize_struct("PluginRequestEnvelope", 3)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("request_id", &self.request_id)?;
        state.serialize_field(
            "request",
            &VersionedRequest { version: self.protocol_version, request: &self.request },
        )?;
        state.end()
    }
}

struct VersionedRequest<'a> {
    version: u16,
    request: &'a HostRequest,
}

impl serde::Serialize for VersionedRequest<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_request(self.request, self.version, serializer)
    }
}

impl<'de> Deserialize<'de> for PluginRequestEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = RequestEnvelopeWire::deserialize(deserializer)?;
        if !matches!(wire.protocol_version, LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION) {
            return Err(de::Error::custom("unsupported plugin request protocol version"));
        }
        let request = decode_request::<D::Error>(wire.request, wire.protocol_version)?;
        validate_request(wire.protocol_version, &request).map_err(D::Error::custom)?;
        Ok(Self {
            protocol_version: wire.protocol_version,
            request_id: wire.request_id,
            request,
        })
    }
}

fn validate_request(protocol_version: u16, request: &HostRequest) -> Result<(), String> {
    match request {
        HostRequest::Initialize { protocol_version: selected, quotas, .. } => {
            if *selected != protocol_version {
                return Err(
                    "initialize protocol version differs from its request envelope".to_owned(),
                );
            }
            quotas.validate().map_err(|error| error.to_string())?;
            if protocol_version == LEGACY_PROTOCOL_VERSION && quotas.legacy_values().is_none() {
                return Err("protocol version one requires finite numeric quotas".to_owned());
            }
        }
        HostRequest::Invoke { capability, context, .. } => {
            if capability.is_empty()
                || context.session_id.is_empty()
                || context.actor_id.is_empty()
                || context.deadline_millis == Some(0)
                || !context.granted_capabilities.iter().any(|granted| granted == capability)
                || context
                    .granted_capabilities
                    .windows(2)
                    .any(|pair| pair[0] >= pair[1])
            {
                return Err(
                    "invocation context is malformed or lacks exact authority".to_owned(),
                );
            }
            if protocol_version == LEGACY_PROTOCOL_VERSION && context.deadline_millis.is_none() {
                return Err("protocol version one requires an invocation deadline".to_owned());
            }
        }
        HostRequest::Cancel { request_id, .. } => {
            let _ = request_id;
        }
        HostRequest::Health | HostRequest::Shutdown => {}
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestEnvelopeWire {
    protocol_version: u16,
    request_id: RequestId,
    request: Value,
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
struct LegacyInitializeBody {
    protocol_version: u16,
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    quotas: LegacyPluginQuotas,
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
struct LegacyInvokeBody {
    capability: String,
    input: JsonPayload,
    context: LegacyInvocationContext,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyInvocationContext {
    session_id: String,
    actor_id: String,
    role: PluginRole,
    granted_capabilities: Vec<String>,
    authority_generation: u64,
    deadline_millis: u64,
}

impl LegacyInvocationContext {
    fn migrate(self) -> InvocationContext {
        InvocationContext {
            session_id: self.session_id,
            actor_id: self.actor_id,
            role: self.role,
            granted_capabilities: self.granted_capabilities,
            authority_generation: self.authority_generation,
            deadline_millis: Some(self.deadline_millis),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPluginQuotas {
    concurrent_requests: u16,
    frame_bytes: u32,
    output_bytes: u64,
    invocation_millis: u64,
    lifecycle_requests: u64,
    protocol_violations: u16,
}

impl LegacyPluginQuotas {
    const fn migrate(self) -> PluginQuotas {
        PluginQuotas {
            concurrent_requests: self.concurrent_requests,
            frame_bytes: self.frame_bytes,
            output_bytes: self.output_bytes,
            invocation_millis: Some(self.invocation_millis),
            lifecycle_requests: CumulativeQuota::Limited {
                limit: self.lifecycle_requests,
            },
            protocol_violations: CumulativeQuota::Limited {
                limit: self.protocol_violations as u64,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelBody {
    request_id: RequestId,
    reason: String,
}
