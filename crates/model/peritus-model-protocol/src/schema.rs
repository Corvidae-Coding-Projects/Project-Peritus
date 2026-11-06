//! Bounded canonical JSON values and provider-facing JSON Schema documents.

use core::fmt;

use serde_json::Value;

use crate::{ProtocolError, ProtocolErrorKind, ProtocolLimits};

/// Independent recursive JSON parsing ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "max_ distinguishes immutable ceilings from observed JSON measurements"
)]
pub struct JsonBounds {
    max_bytes: usize,
    max_depth: usize,
    max_members: usize,
    max_string_bytes: usize,
}

impl JsonBounds {
    /// Derives schema bounds from a protocol limit set.
    #[must_use]
    pub const fn schema(limits: ProtocolLimits) -> Self {
        Self {
            max_bytes: limits.max_schema_bytes(),
            max_depth: 64,
            max_members: u32::MAX as usize,
            max_string_bytes: limits.max_text_bytes(),
        }
    }

    /// Derives structured-value bounds from a protocol limit set.
    #[must_use]
    pub const fn value(limits: ProtocolLimits) -> Self {
        Self {
            max_bytes: limits.max_tool_argument_bytes(),
            max_depth: 64,
            max_members: u32::MAX as usize,
            max_string_bytes: limits.max_text_bytes(),
        }
    }

    /// Creates nonzero ceilings no wider than the production protocol.
    ///
    /// # Errors
    ///
    /// Rejects zero fields and bounds wider than the corresponding production limit.
    pub fn new(
        max_bytes: usize,
        max_depth: usize,
        max_members: usize,
        max_string_bytes: usize,
    ) -> Result<Self, ProtocolError> {
        let production = Self::value(ProtocolLimits::PRODUCTION);
        if max_bytes == 0
            || max_depth == 0
            || max_members == 0
            || max_string_bytes == 0
            || max_bytes > production.max_bytes
            || max_depth > production.max_depth
            || max_members > production.max_members
            || max_string_bytes > production.max_string_bytes
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "json_bounds",
                "JSON bounds must be nonzero and within production ceilings",
            ));
        }
        Ok(Self { max_bytes, max_depth, max_members, max_string_bytes })
    }

    /// Maximum canonical bytes.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

/// A recursively bounded JSON value with deterministic object-key order.
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalJson {
    value: Value,
    canonical: Vec<u8>,
}

impl CanonicalJson {
    /// Parses JSON, rejects duplicate keys, validates recursive bounds, and canonicalizes it.
    ///
    /// # Errors
    ///
    /// Rejects malformed JSON, duplicate keys, exceeded bounds, and noncanonical oversized output.
    pub fn parse(input: &str, bounds: JsonBounds) -> Result<Self, ProtocolError> {
        if input.len() > bounds.max_bytes {
            return Err(invalid("$", "JSON input exceeds its byte bound"));
        }
        let value: Value =
            serde_json::from_str(input).map_err(|_| invalid("$", "JSON syntax is malformed"))?;
        if crate::json_duplicates::contains(input) {
            return Err(invalid("$", "JSON object contains a duplicate key"));
        }
        validate(&value, bounds, 1, &mut 0, "$")?;
        let mut canonical = Vec::with_capacity(input.len());
        write_value(&value, &mut canonical);
        if canonical.len() > bounds.max_bytes {
            return Err(invalid("$", "canonical JSON exceeds its byte bound"));
        }
        Ok(Self { value, canonical })
    }

    /// Borrows compact canonical JSON bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns a wire-ready canonical JSON string.
    #[must_use]
    pub fn to_wire_string(&self) -> String {
        String::from_utf8_lossy(&self.canonical).into_owned()
    }

    /// Returns whether the root value is an object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        self.value.is_object()
    }

    /// Computes a digest of the canonical representation.
    #[must_use]
    pub fn digest(&self) -> peritus_types::Sha256Digest {
        peritus_codec::sha256(&self.canonical)
    }
}

#[allow(
    clippy::missing_fields_in_debug,
    reason = "the parsed value is sensitive and canonical byte count is the complete safe view"
)]
impl fmt::Debug for CanonicalJson {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CanonicalJson")
            .field("bytes", &self.canonical.len())
            .field("content", &"[redacted]")
            .finish()
    }
}

/// JSON Schema family expected by a selected provider model profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaDialect {
    /// JSON Schema Draft 2020-12.
    Draft202012,
    /// JSON Schema Draft 7.
    Draft7,
    /// Google Gemini's documented JSON Schema subset.
    GeminiSubset,
    /// An explicitly profiled compatible-provider subset.
    ProfiledSubset,
}

/// One bounded object-root JSON Schema document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonSchema {
    dialect: SchemaDialect,
    document: CanonicalJson,
}

impl JsonSchema {
    /// Parses and validates a provider-facing schema.
    ///
    /// # Errors
    ///
    /// Rejects non-object roots, malformed/bounded JSON, and remote references.
    pub fn parse(
        input: &str,
        dialect: SchemaDialect,
        bounds: JsonBounds,
    ) -> Result<Self, ProtocolError> {
        let document = CanonicalJson::parse(input, bounds)?;
        if !document.is_object() {
            return Err(invalid("$", "JSON Schema root must be an object"));
        }
        reject_remote_references(&document.value, "$")?;
        Ok(Self { dialect, document })
    }

    /// Returns the selected schema dialect.
    #[must_use]
    pub const fn dialect(&self) -> SchemaDialect {
        self.dialect
    }

    /// Borrows canonical schema bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        self.document.canonical_bytes()
    }

    /// Returns the schema digest.
    #[must_use]
    pub fn digest(&self) -> peritus_types::Sha256Digest {
        self.document.digest()
    }
}

fn validate(
    value: &Value,
    bounds: JsonBounds,
    depth: usize,
    members: &mut usize,
    path: &str,
) -> Result<(), ProtocolError> {
    if depth > bounds.max_depth {
        return Err(invalid(path, "JSON depth exceeds its bound"));
    }
    match value {
        Value::String(text) => check_string(text, bounds, path),
        Value::Array(values) => {
            account(members, values.len(), bounds, path)?;
            for (index, child) in values.iter().enumerate() {
                validate(child, bounds, depth + 1, members, &format!("{path}/{index}"))?;
            }
            Ok(())
        }
        Value::Object(values) => {
            account(members, values.len(), bounds, path)?;
            for (key, child) in values {
                check_string(key, bounds, path)?;
                validate(
                    child,
                    bounds,
                    depth + 1,
                    members,
                    &format!("{path}/{}", escape_pointer(key)),
                )?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn reject_remote_references(value: &Value, path: &str) -> Result<(), ProtocolError> {
    match value {
        Value::Object(values) => {
            if values
                .get("$ref")
                .and_then(Value::as_str)
                .is_some_and(|reference| !reference.starts_with('#'))
            {
                return Err(invalid(path, "remote JSON Schema references are not supported"));
            }
            for (key, child) in values {
                reject_remote_references(child, &format!("{path}/{}", escape_pointer(key)))?;
            }
            Ok(())
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_remote_references(child, &format!("{path}/{index}"))?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Ok(()),
    }
}

fn account(
    total: &mut usize,
    additional: usize,
    bounds: JsonBounds,
    path: &str,
) -> Result<(), ProtocolError> {
    *total = total.checked_add(additional).ok_or_else(|| invalid(path, "JSON member overflow"))?;
    if *total > bounds.max_members {
        return Err(invalid(path, "JSON member count exceeds its bound"));
    }
    Ok(())
}

fn check_string(value: &str, bounds: JsonBounds, path: &str) -> Result<(), ProtocolError> {
    if value.len() > bounds.max_string_bytes {
        return Err(invalid(path, "JSON string exceeds its byte bound"));
    }
    Ok(())
}

fn write_value(value: &Value, output: &mut Vec<u8>) {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(false) => output.extend_from_slice(b"false"),
        Value::Bool(true) => output.extend_from_slice(b"true"),
        Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => {
            output.extend_from_slice(
                serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned()).as_bytes(),
            );
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, child) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(child, output);
            }
            output.push(b']');
        }
        Value::Object(values) => {
            output.push(b'{');
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by_key(|(key, _)| *key);
            for (index, (key, child)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_owned()).as_bytes(),
                );
                output.push(b':');
                write_value(child, output);
            }
            output.push(b'}');
        }
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn invalid(path: &str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidSchema, path, detail)
}

#[cfg(test)]
mod tests {
    use super::{CanonicalJson, JsonBounds, JsonSchema, SchemaDialect};
    use crate::ProtocolErrorKind;
    use serde_json::Value;

    fn bounds(bytes: usize, depth: usize, members: usize, string_bytes: usize) -> JsonBounds {
        JsonBounds::new(bytes, depth, members, string_bytes).unwrap()
    }

    #[test]
    fn bounds_accessor_reports_configured_byte_ceiling() {
        let bounds = bounds(123, 4, 8, 64);

        assert_eq!(bounds.max_bytes(), 123);
    }

    #[test]
    fn input_and_canonical_byte_limits_are_inclusive_and_fail_closed() {
        assert!(CanonicalJson::parse("null", bounds(4, 4, 8, 8)).is_ok());
        assert!(CanonicalJson::parse("0", bounds(4, 4, 8, 8)).is_ok());
        assert!(CanonicalJson::parse("null", bounds(3, 4, 8, 8)).is_err());

        // serde_json normalizes this three-byte exponent to a longer number.
        let error = CanonicalJson::parse("1e1", bounds(3, 4, 8, 8)).unwrap_err();
        assert_eq!(error.kind(), ProtocolErrorKind::InvalidSchema);
        assert_eq!(error.path(), "$");
    }

    #[test]
    fn recursive_limits_accept_the_boundary_and_reject_the_next_value() {
        assert!(CanonicalJson::parse(r#"["ab"]"#, bounds(6, 2, 1, 2)).is_ok());

        assert!(CanonicalJson::parse("[[0]]", bounds(64, 2, 16, 16)).is_err());
        assert!(CanonicalJson::parse(r#"{"a":{"b":0}}"#, bounds(64, 2, 16, 16)).is_err());
        assert!(CanonicalJson::parse("[0,1]", bounds(64, 8, 1, 16)).is_err());
        assert!(CanonicalJson::parse(r#"["abc"]"#, bounds(64, 8, 16, 2)).is_err());
        assert!(CanonicalJson::parse(r#"{"abc":0}"#, bounds(64, 8, 16, 2)).is_err());
    }

    #[test]
    fn canonical_projection_accessors_preserve_order_identity_and_redaction() {
        let canonical = CanonicalJson::parse(
            r#" { "z" : [true, false, null], "a" : {"n": 1} } "#,
            bounds(128, 8, 16, 16),
        )
        .unwrap();
        let expected = br#"{"a":{"n":1},"z":[true,false,null]}"#;

        assert_eq!(canonical.canonical_bytes(), expected);
        assert_eq!(canonical.to_wire_string().as_bytes(), expected);
        assert!(canonical.is_object());
        assert_eq!(canonical.digest(), peritus_codec::sha256(expected));

        let debug = format!("{canonical:?}");
        assert!(debug.contains("CanonicalJson"));
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("true"));

        let scalar = CanonicalJson::parse("null", bounds(8, 2, 2, 2)).unwrap();
        assert!(!scalar.is_object());
    }

    #[test]
    fn malformed_and_duplicate_json_never_reaches_canonical_output() {
        assert!(CanonicalJson::parse("{", bounds(8, 4, 4, 4)).is_err());
        assert!(CanonicalJson::parse(r#"{"a":1,"a":2}"#, bounds(32, 4, 4, 4)).is_err());
    }

    #[test]
    fn schema_accessors_and_reference_policy_match_the_selected_document() {
        let input = r##"{"$ref":"#/$defs/x","$defs":{"x":{"type":"string"}}}"##;
        let schema =
            JsonSchema::parse(input, SchemaDialect::Draft7, bounds(128, 8, 16, 32)).unwrap();
        let expected = br##"{"$defs":{"x":{"type":"string"}},"$ref":"#/$defs/x"}"##;

        assert_eq!(schema.dialect(), SchemaDialect::Draft7);
        assert_eq!(schema.canonical_bytes(), expected);
        assert_eq!(schema.digest(), peritus_codec::sha256(expected));

        assert!(JsonSchema::parse("[]", SchemaDialect::Draft7, bounds(8, 4, 4, 4)).is_err());
    }

    #[test]
    fn remote_schema_references_report_an_escaped_nested_path() {
        let error = JsonSchema::parse(
            r#"{"a~/":{"$ref":"https://example.invalid/schema"}}"#,
            SchemaDialect::Draft202012,
            bounds(128, 8, 16, 64),
        )
        .unwrap_err();

        assert_eq!(error.kind(), ProtocolErrorKind::InvalidSchema);
        assert_eq!(error.path(), "$/a~0~1");
        assert_eq!(error.detail(), "remote JSON Schema references are not supported");
    }

    #[test]
    fn canonical_number_reparses_without_changing_value() {
        let bounds = bounds(65_536, 32, 4_096, 32_768);
        let input = "4341467868529652095759";
        let original = serde_json::from_str::<Value>(input).unwrap();
        let canonical = CanonicalJson::parse(input, bounds).unwrap();
        let wire = canonical.to_wire_string();

        assert_eq!(serde_json::from_str::<Value>(&wire).unwrap(), original);
        assert_eq!(CanonicalJson::parse(&wire, bounds).unwrap(), canonical);
    }
}
