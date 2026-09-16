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

    #[test]
    fn nested_objects_cannot_bypass_the_outer_response_schema() {
        for value in [
            r#"{"value":"outer","extra":{"value":"nested"}}"#,
            r#"{"wrapper":{"value":"nested"}}"#,
            r#"[{"value":"nested"}]"#,
            r#"{"value":"first","value":"second"}"#,
        ] {
            assert!(matches!(
                last_typed_object::<Expected>(value),
                Err(TypedObjectError::Invalid(_))
            ));
        }
    }

    #[test]
    fn later_response_after_nested_examples_preserves_its_literal_text() {
        let response = r#"Example: [{"value":"example"}].
Final: {"value":"literal {\"value\":\"quoted\"}"}"#;

        assert_eq!(
            last_typed_object::<Expected>(response).expect("strict final object"),
            Expected { value: r#"literal {"value":"quoted"}"#.to_owned() }
        );
    }
}
