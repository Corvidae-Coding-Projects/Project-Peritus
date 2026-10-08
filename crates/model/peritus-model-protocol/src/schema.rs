//! Bounded canonical JSON values and provider-facing JSON Schema documents.

use core::fmt;

use crate::{ProtocolError, ProtocolErrorKind, ProtocolLimits};

/// Independent JSON parsing ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "max_ distinguishes immutable ceilings from observed JSON measurements"
)]
pub struct JsonBounds {
    max_bytes: usize,
    max_depth: Option<usize>,
    max_members: Option<usize>,
    max_string_bytes: usize,
}

impl JsonBounds {
    /// Derives schema bounds from a protocol limit set.
    #[must_use]
    pub const fn schema(limits: ProtocolLimits) -> Self {
        Self {
            max_bytes: limits.max_schema_bytes(),
            max_depth: None,
            max_members: None,
            max_string_bytes: limits.max_text_bytes(),
        }
    }

    /// Derives structured-value bounds from a protocol limit set.
    #[must_use]
    pub const fn value(limits: ProtocolLimits) -> Self {
        Self {
            max_bytes: limits.max_tool_argument_bytes(),
            max_depth: None,
            max_members: None,
            max_string_bytes: limits.max_text_bytes(),
        }
    }

    /// Creates nonzero byte ceilings and explicit structural policy limits.
    ///
    /// # Errors
    ///
    /// Rejects zero fields and byte bounds wider than the production protocol.
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
            || max_string_bytes > production.max_string_bytes
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "json_bounds",
                "JSON bounds must be nonzero and byte bounds must be within production ceilings",
            ));
        }
        Ok(Self {
            max_bytes,
            max_depth: Some(max_depth),
            max_members: Some(max_members),
            max_string_bytes,
        })
    }

    /// Maximum canonical bytes.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

/// A bounded canonical JSON value with deterministic object-key order.
#[derive(Clone, Eq, PartialEq)]
pub struct CanonicalJson {
    canonical: Vec<u8>,
    root_is_object: bool,
    remote_reference: Option<String>,
}

impl CanonicalJson {
    /// Parses JSON, rejects duplicate keys, enforces selected bounds, and canonicalizes it.
    ///
    /// # Errors
    ///
    /// Rejects malformed JSON, duplicate keys, exceeded bounds, and noncanonical oversized output.
    pub fn parse(input: &str, bounds: JsonBounds) -> Result<Self, ProtocolError> {
        let parsed = crate::json_duplicates::parse(
            input,
            crate::json_duplicates::ParseLimits {
                max_bytes: bounds.max_bytes,
                max_depth: bounds.max_depth,
                max_members: bounds.max_members,
                max_string_bytes: bounds.max_string_bytes,
            },
        )
        .map_err(|error| invalid(error.path(), error.detail()))?;
        Ok(Self {
            canonical: parsed.canonical,
            root_is_object: parsed.root_is_object,
            remote_reference: parsed.remote_reference,
        })
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
    pub const fn is_object(&self) -> bool {
        self.root_is_object
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
        if let Some(path) = document.remote_reference.as_deref() {
            return Err(invalid(path, "remote JSON Schema references are not supported"));
        }
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
