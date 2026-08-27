//! Strict JSON-RPC 2.0 request and response envelopes.

use serde_json::Value;

/// JSON-RPC request identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RpcId {
    /// String identifier.
    String(String),
    /// Signed integer identifier.
    Number(i64),
}

impl serde::Serialize for RpcId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::String(value) => serializer.serialize_str(value),
            Self::Number(value) => serializer.serialize_i64(*value),
        }
    }
}

impl<'de> serde::Deserialize<'de> for RpcId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match <Value as serde::Deserialize>::deserialize(deserializer)? {
            Value::String(value) => Ok(Self::String(value)),
            Value::Number(value) => value.as_i64().map(Self::Number).ok_or_else(|| {
                <D::Error as serde::de::Error>::custom(
                    "JSON-RPC id number must be a signed integer",
                )
            }),
            _ => Err(<D::Error as serde::de::Error>::custom(
                "JSON-RPC id must be a string or signed integer",
            )),
        }
    }
}

/// Strict JSON-RPC request or notification.
#[derive(Clone, Debug)]
pub struct JsonRpcRequest {
    /// Must equal `2.0`.
    pub jsonrpc: String,
    /// Absent for a notification.
    pub id: Option<RpcId>,
    /// Method name.
    pub method: String,
    /// Optional method parameters.
    pub params: Option<Value>,
}

impl<'de> serde::Deserialize<'de> for JsonRpcRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <Value as serde::Deserialize>::deserialize(deserializer)?;
        let Value::Object(mut fields) = value else {
            return Err(<D::Error as serde::de::Error>::custom(
                "JSON-RPC request must be an object",
            ));
        };
        let jsonrpc = required::<String, D::Error>(&mut fields, "jsonrpc")?;
        let id = optional::<RpcId, D::Error>(&mut fields, "id")?;
        let method = required::<String, D::Error>(&mut fields, "method")?;
        let params = optional::<Value, D::Error>(&mut fields, "params")?;
        if let Some(name) = fields.keys().next() {
            return Err(<D::Error as serde::de::Error>::custom(format!(
                "unknown JSON-RPC request field: {name}"
            )));
        }
        Ok(Self { jsonrpc, id, method, params })
    }
}

/// JSON-RPC error object.
#[derive(Clone, Debug)]
pub struct JsonRpcError {
    /// Numeric JSON-RPC or MCP error code.
    pub code: i32,
    /// Stable bounded message.
    pub message: String,
    /// Optional structured detail.
    pub data: Option<Value>,
}

impl serde::Serialize for JsonRpcError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2 + usize::from(self.data.is_some())))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "code", &self.code)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "message", &self.message)?;
        if let Some(data) = &self.data {
            serde::ser::SerializeMap::serialize_entry(&mut map, "data", data)?;
        }
        serde::ser::SerializeMap::end(map)
    }
}

/// Strict JSON-RPC success or error response.
#[derive(Clone, Debug)]
pub struct JsonRpcResponse {
    /// Exact JSON-RPC version.
    pub jsonrpc: &'static str,
    /// Correlated request identity, or null for parse errors.
    pub id: Option<RpcId>,
    /// Successful result.
    pub result: Option<Value>,
    /// Failure result.
    pub error: Option<JsonRpcError>,
}

impl serde::Serialize for JsonRpcResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(
            2 + usize::from(self.result.is_some()) + usize::from(self.error.is_some()),
        ))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "jsonrpc", self.jsonrpc)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "id", &self.id)?;
        if let Some(result) = &self.result {
            serde::ser::SerializeMap::serialize_entry(&mut map, "result", result)?;
        }
        if let Some(error) = &self.error {
            serde::ser::SerializeMap::serialize_entry(&mut map, "error", error)?;
        }
        serde::ser::SerializeMap::end(map)
    }
}

impl JsonRpcResponse {
    /// Creates a successful response.
    #[must_use]
    pub const fn success(id: RpcId, result: Value) -> Self {
        Self { jsonrpc: "2.0", id: Some(id), result: Some(result), error: None }
    }

    /// Creates an error response.
    #[must_use]
    pub fn failure(id: Option<RpcId>, code: i32, message: impl Into<String>) -> Self {
        let mut message = message.into();
        message.truncate(512);
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError { code, message, data: None }),
        }
    }

    /// Creates an error response with structured detail.
    #[must_use]
    pub fn failure_with_data(
        id: Option<RpcId>,
        code: i32,
        message: impl Into<String>,
        data: Value,
    ) -> Self {
        let mut response = Self::failure(id, code, message);
        if let Some(error) = &mut response.error {
            error.data = Some(data);
        }
        response
    }
}

fn required<T, E>(fields: &mut serde_json::Map<String, Value>, name: &'static str) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    let value = fields
        .remove(name)
        .ok_or_else(|| E::custom(format!("missing required JSON-RPC field: {name}")))?;
    serde_json::from_value(value)
        .map_err(|error| E::custom(format!("invalid JSON-RPC field {name}: {error}")))
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
            .map_err(|error| E::custom(format!("invalid JSON-RPC field {name}: {error}"))),
    }
}
