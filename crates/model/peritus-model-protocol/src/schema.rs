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
            max_members: 65_536,
            max_string_bytes: limits.max_text_bytes(),
        }
    }

    /// Derives structured-value bounds from a protocol limit set.
    #[must_use]
    pub const fn value(limits: ProtocolLimits) -> Self {
        Self {
            max_bytes: limits.max_tool_argument_bytes(),
            max_depth: 64,
            max_members: 65_536,
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
    use super::*;

    #[test]
    fn canonicalizes_object_order_and_rejects_duplicates() {
        let bounds = JsonBounds::schema(ProtocolLimits::PRODUCTION);
        let value = CanonicalJson::parse(r#"{"z":1,"a":[true,null]}"#, bounds).expect("valid JSON");
        assert_eq!(value.canonical_bytes(), br#"{"a":[true,null],"z":1}"#);
        assert_eq!(
            CanonicalJson::parse(r#"{"a":1,"a":2}"#, bounds).expect_err("duplicate").kind(),
            ProtocolErrorKind::InvalidSchema
        );
    }

    #[test]
    fn schema_rejects_remote_reference() {
        let error = JsonSchema::parse(
            r#"{"$ref":"https://example.invalid/schema"}"#,
            SchemaDialect::Draft202012,
            JsonBounds::schema(ProtocolLimits::PRODUCTION),
        )
        .expect_err("remote reference");
        assert_eq!(error.kind(), ProtocolErrorKind::InvalidSchema);
    }
}
