//! Shared strict extraction of typed JSON objects from model responses.

use serde::de::{DeserializeOwned, IgnoredAny};

/// Failure to find or decode the final object matching a response contract.
#[derive(Debug)]
pub enum TypedObjectError {
    /// The response contains no object start.
    Missing,
    /// Object-shaped content exists, but none matches the requested strict type.
    Invalid(String),
}

/// Selects the last structurally valid top-level object matching `T`.
///
/// Explanatory prose and earlier brace-delimited examples are allowed without widening the typed
/// response schema. Valid containers are consumed whole so their nested objects cannot masquerade
/// as a response after the outer container fails the contract.
pub fn last_typed_object<T: DeserializeOwned>(value: &str) -> Result<T, TypedObjectError> {
    let mut found_object_start = false;
    let mut last_error = None;
    let mut container_end = 0;
    let mut candidate = None;
    for (start, character) in value.char_indices() {
        found_object_start |= character == '{';
        if start < container_end || !matches!(character, '{' | '[') {
            continue;
        }
        let mut values =
            serde_json::Deserializer::from_str(&value[start..]).into_iter::<IgnoredAny>();
        match values.next() {
            Some(Ok(_)) => {
                container_end = start + values.byte_offset();
                if character == '{' {
                    match serde_json::from_str::<T>(&value[start..container_end]) {
                        Ok(wire) => candidate = Some(wire),
                        Err(error) => last_error = Some(error.to_string()),
                    }
                } else {
                    last_error =
                        Some("model response requires a JSON object, not an array".to_owned());
                }
            }
            Some(Err(error)) => last_error = Some(error.to_string()),
            None => {}
        }
    }
    if let Some(candidate) = candidate {
        return Ok(candidate);
    }
    if !found_object_start {
        return Err(TypedObjectError::Missing);
    }
    Err(TypedObjectError::Invalid(
        last_error.unwrap_or_else(|| "model response has incomplete JSON".to_owned()),
    ))
}
