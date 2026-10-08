//! Version-aware request encoding without intermediate JSON value trees.

use serde::{
    Deserialize,
    de,
    ser::{Error as _, SerializeMap, SerializeStruct},
};
use serde_json::value::RawValue;

use super::{
    HostRequest, InvocationContext, LEGACY_PROTOCOL_VERSION, PROTOCOL_VERSION,
    PluginRequestEnvelope, PluginRole,
};
use crate::{
    CumulativeQuota, JsonPayload, JsonStructure, PluginId, PluginQuotas, PluginVersion, RequestId,
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
        self.validate().map_err(S::Error::custom)?;
        let fields = 5 + usize::from(self.deadline_millis.is_some());
        let mut state = serializer.serialize_struct("InvocationContext", fields)?;
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
    match request {
        HostRequest::Initialize {
            protocol_version,
            plugin_id,
            plugin_version,
            quotas,
        } => {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("method", "initialize")?;
            if wire_version == LEGACY_PROTOCOL_VERSION {
                map.serialize_entry(
                    "params",
                    &LegacyInitializeRef {
                        protocol_version: *protocol_version,
                        plugin_id,
                        plugin_version: *plugin_version,
                        quotas: LegacyQuotasRef(quotas),
                    },
                )?;
            } else {
                map.serialize_entry(
                    "params",
                    &InitializeRef {
                        protocol_version: *protocol_version,
                        plugin_id,
                        plugin_version: *plugin_version,
                        quotas,
                    },
                )?;
            }
            map.end()
        }
        HostRequest::Invoke { capability, input, context } => {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("method", "invoke")?;
            if wire_version == LEGACY_PROTOCOL_VERSION {
                map.serialize_entry(
                    "params",
                    &LegacyInvokeRef { capability, input, context: LegacyContextRef(context) },
                )?;
            } else {
                map.serialize_entry("params", &InvokeRef { capability, input, context })?;
            }
            map.end()
        }
        HostRequest::Cancel { request_id, reason } => {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("method", "cancel")?;
            map.serialize_entry("params", &CancelRef { request_id, reason })?;
            map.end()
        }
        HostRequest::Health => serialize_unit_request(serializer, "health"),
        HostRequest::Shutdown => serialize_unit_request(serializer, "shutdown"),
    }
}

fn serialize_unit_request<S>(serializer: S, method: &'static str) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry("method", method)?;
    map.end()
}

struct InitializeRef<'a> {
    protocol_version: u16,
    plugin_id: &'a PluginId,
    plugin_version: PluginVersion,
    quotas: &'a PluginQuotas,
}

impl serde::Serialize for InitializeRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("InitializeBody", 4)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("plugin_id", self.plugin_id)?;
        state.serialize_field("plugin_version", &self.plugin_version)?;
        state.serialize_field("quotas", self.quotas)?;
        state.end()
    }
}

struct LegacyInitializeRef<'a> {
    protocol_version: u16,
    plugin_id: &'a PluginId,
    plugin_version: PluginVersion,
    quotas: LegacyQuotasRef<'a>,
}

impl serde::Serialize for LegacyInitializeRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("InitializeBody", 4)?;
        state.serialize_field("protocol_version", &self.protocol_version)?;
        state.serialize_field("plugin_id", self.plugin_id)?;
        state.serialize_field("plugin_version", &self.plugin_version)?;
        state.serialize_field("quotas", &self.quotas)?;
        state.end()
    }
}

struct InvokeRef<'a> {
    capability: &'a str,
    input: &'a JsonPayload,
    context: &'a InvocationContext,
}

impl serde::Serialize for InvokeRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("InvokeBody", 3)?;
        state.serialize_field("capability", self.capability)?;
        state.serialize_field("input", self.input)?;
        state.serialize_field("context", self.context)?;
        state.end()
    }
}

struct LegacyInvokeRef<'a> {
    capability: &'a str,
    input: &'a JsonPayload,
    context: LegacyContextRef<'a>,
}

impl serde::Serialize for LegacyInvokeRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("InvokeBody", 3)?;
        state.serialize_field("capability", self.capability)?;
        state.serialize_field("input", self.input)?;
        state.serialize_field("context", &self.context)?;
        state.end()
    }
}

struct CancelRef<'a> {
    request_id: &'a RequestId,
    reason: &'a str,
}

impl serde::Serialize for CancelRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("CancelBody", 2)?;
        state.serialize_field("request_id", self.request_id)?;
        state.serialize_field("reason", self.reason)?;
        state.end()
    }
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
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        decode_request(&raw, PROTOCOL_VERSION)
    }
}

fn decode_request<E>(raw: &RawValue, protocol_version: u16) -> Result<HostRequest, E>
where
    E: de::Error,
{
    let wire: TaggedRequest<'_> = decode_raw(raw, "plugin request")?;
    match wire.method.as_str() {
        "initialize" => {
            let params = required_content(wire.params.value, "params")?;
            if protocol_version == LEGACY_PROTOCOL_VERSION {
                let body: LegacyInitializeBody = decode_raw(params, "initialize params")?;
                Ok(HostRequest::Initialize {
                    protocol_version: body.protocol_version,
                    plugin_id: body.plugin_id,
                    plugin_version: body.plugin_version,
                    quotas: body.quotas.migrate(),
                })
            } else {
                let body: InitializeBody = decode_raw(params, "initialize params")?;
                Ok(HostRequest::Initialize {
                    protocol_version: body.protocol_version,
                    plugin_id: body.plugin_id,
                    plugin_version: body.plugin_version,
                    quotas: body.quotas,
                })
            }
        }
        "invoke" => {
            let params = required_content(wire.params.value, "params")?;
            if protocol_version == LEGACY_PROTOCOL_VERSION {
                let body: LegacyInvokeBody<'_> = decode_raw(params, "invoke params")?;
                Ok(HostRequest::Invoke {
                    capability: body.capability,
                    input: JsonPayload::parse_active(body.input.get().as_bytes())
                        .map_err(E::custom)?,
                    context: body.context.migrate().map_err(E::custom)?,
                })
            } else {
                let body: InvokeBody<'_> = decode_raw(params, "invoke params")?;
                Ok(HostRequest::Invoke {
                    capability: body.capability,
                    input: JsonPayload::parse_active(body.input.get().as_bytes())
                        .map_err(E::custom)?,
                    context: body.context,
                })
            }
        }
        "cancel" => {
            let body: CancelBody = decode_raw(
                required_content(wire.params.value, "params")?,
                "cancel params",
            )?;
            Ok(HostRequest::Cancel { request_id: body.request_id, reason: body.reason })
        }
        "health" => {
            require_unit(wire.params.value, "health")?;
            Ok(HostRequest::Health)
        }
        "shutdown" => {
            require_unit(wire.params.value, "shutdown")?;
            Ok(HostRequest::Shutdown)
        }
        _ => Err(E::unknown_variant(
            &wire.method,
            &["initialize", "invoke", "cancel", "health", "shutdown"],
        )),
    }
}

impl serde::Serialize for PluginRequestEnvelope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.protocol_version == LEGACY_PROTOCOL_VERSION
            && !self.request_id.is_v1_compatible()
        {
            return Err(S::Error::custom(
                "protocol version one request identifier is not representable",
            ));
        }
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
        if wire.protocol_version == LEGACY_PROTOCOL_VERSION
            && !wire.request_id.is_v1_compatible()
        {
            return Err(de::Error::custom(
                "protocol version one request identifier is not representable",
            ));
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
        HostRequest::Initialize {
            protocol_version: selected,
            plugin_id,
            plugin_version,
            quotas,
            ..
        } => {
            if *selected != protocol_version {
                return Err(
                    "initialize protocol version differs from its request envelope".to_owned(),
                );
            }
            quotas.validate().map_err(|error| error.to_string())?;
            if protocol_version == LEGACY_PROTOCOL_VERSION
                && (!plugin_id.is_v1_compatible()
                    || !plugin_version.is_v1_compatible()
                    || quotas.legacy_values().is_none())
            {
                return Err(
                    "protocol version one requires representable version and quota fields"
                        .to_owned(),
                );
            }
        }
        HostRequest::Invoke { capability, context, .. } => {
            crate::manifest::validate_capability_name(capability)
                .map_err(|error| error.to_string())?;
            context.validate().map_err(|error| error.to_string())?;
            if protocol_version == LEGACY_PROTOCOL_VERSION
                && (!crate::manifest::is_v1_capability_name(capability)
                    || context
                        .granted_capabilities
                        .iter()
                        .any(|name| !crate::manifest::is_v1_capability_name(name)))
            {
                return Err(
                    "protocol version one capability identity is not representable".to_owned(),
                );
            }
            if !context.granted_capabilities.iter().any(|granted| granted == capability) {
                return Err(
                    "invocation context is malformed or lacks exact authority".to_owned(),
                );
            }
            if protocol_version == LEGACY_PROTOCOL_VERSION && context.deadline_millis.is_none() {
                return Err("protocol version one requires an invocation deadline".to_owned());
            }
        }
        HostRequest::Cancel { request_id, .. } => {
            if protocol_version == LEGACY_PROTOCOL_VERSION && !request_id.is_v1_compatible() {
                return Err(
                    "protocol version one cancellation identity is not representable".to_owned(),
                );
            }
        }
        HostRequest::Health | HostRequest::Shutdown => {}
    }
    Ok(())
}

fn decode_raw<'a, T, E>(raw: &'a RawValue, label: &'static str) -> Result<T, E>
where
    T: Deserialize<'a>,
    E: de::Error,
{
    serde_json::from_str(raw.get())
        .map_err(|error| E::custom(format!("malformed {label}: {error}")))
}

fn required_content<'a, E: de::Error>(
    content: Option<&'a RawValue>,
    name: &'static str,
) -> Result<&'a RawValue, E> {
    content.ok_or_else(|| E::missing_field(name))
}

fn require_unit<E: de::Error>(
    content: Option<&RawValue>,
    variant: &'static str,
) -> Result<(), E> {
    if content.is_some() {
        Err(E::custom(format!("{variant} must not contain protocol content")))
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestEnvelopeWire<'a> {
    protocol_version: u16,
    request_id: RequestId,
    #[serde(borrow)]
    request: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaggedRequest<'a> {
    method: String,
    #[serde(borrow, default)]
    params: OptionalRaw<'a>,
}

#[derive(Default)]
struct OptionalRaw<'a> {
    value: Option<&'a RawValue>,
}

impl<'de: 'a, 'a> Deserialize<'de> for OptionalRaw<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: &'de RawValue = <&RawValue>::deserialize(deserializer)?;
        Ok(Self { value: Some(value) })
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
struct LegacyInitializeBody {
    protocol_version: u16,
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    quotas: LegacyPluginQuotas,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvokeBody<'a> {
    capability: String,
    #[serde(borrow)]
    input: &'a RawValue,
    context: InvocationContext,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyInvokeBody<'a> {
    capability: String,
    #[serde(borrow)]
    input: &'a RawValue,
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
    fn migrate(self) -> Result<InvocationContext, crate::SdkError> {
        InvocationContext::new(
            self.session_id,
            self.actor_id,
            self.role,
            self.granted_capabilities,
            self.authority_generation,
            Some(self.deadline_millis),
        )
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
            json: JsonStructure::V1_COMPATIBILITY,
            invocation_millis: Some(self.invocation_millis),
            lifecycle_requests: CumulativeQuota::Limited { limit: self.lifecycle_requests },
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
