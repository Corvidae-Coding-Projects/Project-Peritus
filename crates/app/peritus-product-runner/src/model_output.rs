//! Shared strict extraction of typed JSON objects from model responses.

use serde::de::DeserializeOwned;

/// Failure to find or decode the final object matching a response contract.
#[derive(Debug)]
pub enum TypedObjectError {
    /// The response contains no object start.
    Missing,
    /// Object-shaped content exists, but none matches the requested strict type.
    Invalid(String),
}

/// Selects the last structurally valid object matching `T`.
///
/// Scanning from the end allows explanatory prose and earlier brace-delimited examples without
/// widening the typed response schema.
pub fn last_typed_object<T: DeserializeOwned>(value: &str) -> Result<T, TypedObjectError> {
    let mut found_object_start = false;
    let mut last_error = None;
    for (start, character) in value.char_indices().rev() {
        if character != '{' {
            continue;
        }
        found_object_start = true;
        let mut values = serde_json::Deserializer::from_str(&value[start..]).into_iter::<T>();
        match values.next() {
            Some(Ok(wire)) => return Ok(wire),
            Some(Err(error)) => last_error = Some(error),
            None => {}
        }
    }
    if !found_object_start {
        return Err(TypedObjectError::Missing);
    }
    Err(TypedObjectError::Invalid(last_error.map_or_else(
        || "model response has incomplete JSON".to_owned(),
        |error| error.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Expected {
        value: String,
    }

    #[test]
    fn last_strict_object_skips_prose_examples_and_unknown_shapes() {
        let response = r#"Observed {'value': 'python'} and {"other":"json"}.
Final: {"value":"accepted"}"#;

        assert_eq!(
            last_typed_object::<Expected>(response).expect("strict final object"),
            Expected { value: "accepted".to_owned() }
        );
    }

    #[test]
    fn missing_and_mismatched_objects_remain_distinct() {
        assert!(matches!(
            last_typed_object::<Expected>("plain response"),
            Err(TypedObjectError::Missing)
        ));
        assert!(matches!(
            last_typed_object::<Expected>(r#"{"other":"json"}"#),
            Err(TypedObjectError::Invalid(_))
        ));
    }
}
