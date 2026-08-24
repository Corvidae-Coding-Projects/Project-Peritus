//! Small explicit JSON constructors for Anthropic's private request wire model.

use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::invalid;

pub fn json_value(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes)
        .map_err(|_| invalid("validated canonical JSON could not be projected to Anthropic"))
}

pub fn millionths(value: u32) -> f64 {
    f64::from(value) / 1_000_000.0
}

pub fn choice_value(kind: &str, disabled: bool, name: Option<&str>) -> Value {
    let mut value = Map::new();
    value.insert("type".to_owned(), wire_string(kind));
    value.insert("disable_parallel_tool_use".to_owned(), Value::Bool(disabled));
    if let Some(name) = name {
        value.insert("name".to_owned(), wire_string(name));
    }
    Value::Object(value)
}

pub fn typed_value(kind: &str) -> Value {
    wire_object([("type", wire_string(kind))])
}

pub fn wire_object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect())
}

pub fn wire_string(value: &str) -> Value {
    Value::String(value.to_owned())
}
