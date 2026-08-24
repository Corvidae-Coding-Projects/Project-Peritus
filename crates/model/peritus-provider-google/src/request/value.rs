//! Explicit JSON construction for the private Google wire boundary.

use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::invalid;

pub(super) fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Object(fields.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

pub(super) fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub(super) fn parse(bytes: &[u8]) -> Result<Value, ProviderCoreError> {
    serde_json::from_slice(bytes)
        .map_err(|_| invalid("validated canonical JSON could not be projected to Google"))
}

pub(super) fn insert(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), value);
    }
}

pub(super) fn millionths(value: u32) -> Value {
    Value::from(f64::from(value) / 1_000_000.0)
}
