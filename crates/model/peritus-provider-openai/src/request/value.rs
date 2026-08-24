//! Explicit JSON value construction without code-generating macros.

use serde_json::{Map, Value};

pub(super) fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Object(
        entries.into_iter().map(|(name, value)| (name.to_owned(), value)).collect::<Map<_, _>>(),
    )
}

pub(super) fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub(super) fn optional_string(value: Option<&str>) -> Value {
    value.map_or(Value::Null, string)
}
