//! MCP wire DTOs and method parameter validation.

use serde_json::Value;

use crate::RpcId;

/// Supported MCP protocol version.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Client implementation identity from `initialize`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpClientInfo {
    /// Client name.
    pub name: String,
    /// Client version.
    pub version: String,
}

impl<'de> serde::Deserialize<'de> for McpClientInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "clientInfo")?;
        let name = required::<String, D::Error>(&mut fields, "name")?;
        let version = required::<String, D::Error>(&mut fields, "version")?;
        finish::<D::Error>(&fields, "clientInfo")?;
        Ok(Self { name, version })
    }
}

/// Server implementation identity returned by `initialize`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct McpServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
}

impl serde::Serialize for McpServerInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "name", &self.name)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "version", &self.version)?;
        serde::ser::SerializeMap::end(map)
    }
}

#[derive(Debug)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: Value,
    pub client_info: McpClientInfo,
}

impl<'de> serde::Deserialize<'de> for InitializeParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "initialize params")?;
        let protocol_version = required::<String, D::Error>(&mut fields, "protocolVersion")?;
        let capabilities = value_or(&mut fields, "capabilities", Value::Null);
        let client_info = required::<McpClientInfo, D::Error>(&mut fields, "clientInfo")?;
        finish::<D::Error>(&fields, "initialize params")?;
        Ok(Self { protocol_version, capabilities, client_info })
    }
}

#[derive(Debug)]
pub struct CursorParams {
    pub cursor: Option<String>,
}

impl<'de> serde::Deserialize<'de> for CursorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "cursor params")?;
        let cursor = optional::<String, D::Error>(&mut fields, "cursor")?;
        finish::<D::Error>(&fields, "cursor params")?;
        Ok(Self { cursor })
    }
}

#[derive(Debug)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Value,
}

impl<'de> serde::Deserialize<'de> for ToolCallParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "tool call params")?;
        let name = required::<String, D::Error>(&mut fields, "name")?;
        let arguments = value_or(&mut fields, "arguments", empty_object());
        finish::<D::Error>(&fields, "tool call params")?;
        Ok(Self { name, arguments })
    }
}

#[derive(Debug)]
pub struct ResourceReadParams {
    pub uri: String,
}

impl<'de> serde::Deserialize<'de> for ResourceReadParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "resource read params")?;
        let uri = required::<String, D::Error>(&mut fields, "uri")?;
        finish::<D::Error>(&fields, "resource read params")?;
        Ok(Self { uri })
    }
}

#[derive(Debug)]
pub struct PromptGetParams {
    pub name: String,
    pub arguments: Value,
}

impl<'de> serde::Deserialize<'de> for PromptGetParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "prompt get params")?;
        let name = required::<String, D::Error>(&mut fields, "name")?;
        let arguments = value_or(&mut fields, "arguments", Value::Null);
        finish::<D::Error>(&fields, "prompt get params")?;
        Ok(Self { name, arguments })
    }
}

#[derive(Debug)]
pub struct CancelParams {
    pub request_id: RpcId,
    pub reason: Option<String>,
}

impl<'de> serde::Deserialize<'de> for CancelParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut fields = object(deserializer, "cancel params")?;
        let request_id = required::<RpcId, D::Error>(&mut fields, "requestId")?;
        let reason = optional::<String, D::Error>(&mut fields, "reason")?;
        finish::<D::Error>(&fields, "cancel params")?;
        Ok(Self { request_id, reason })
    }
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

fn object<'de, D>(
    deserializer: D,
    context: &'static str,
) -> Result<serde_json::Map<String, Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match <Value as serde::Deserialize>::deserialize(deserializer)? {
        Value::Object(fields) => Ok(fields),
        _ => Err(<D::Error as serde::de::Error>::custom(format!("{context} must be an object"))),
    }
}

fn required<T, E>(fields: &mut serde_json::Map<String, Value>, name: &'static str) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    let value =
        fields.remove(name).ok_or_else(|| E::custom(format!("missing required field: {name}")))?;
    serde_json::from_value(value)
        .map_err(|error| E::custom(format!("invalid field {name}: {error}")))
}

fn optional<T, E>(
    fields: &mut serde_json::Map<String, Value>,
    name: &'static str,
) -> Result<Option<T>, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    match fields.remove(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| E::custom(format!("invalid field {name}: {error}"))),
    }
}

fn value_or(
    fields: &mut serde_json::Map<String, Value>,
    name: &'static str,
    default: Value,
) -> Value {
    fields.remove(name).map_or(default, |value| value)
}

fn finish<E>(fields: &serde_json::Map<String, Value>, context: &'static str) -> Result<(), E>
where
    E: serde::de::Error,
{
    fields
        .keys()
        .next()
        .map_or_else(|| Ok(()), |name| Err(E::custom(format!("unknown {context} field: {name}"))))
}
