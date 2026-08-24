//! Deterministic bounded JSON values and the supported schema subset.

mod canonical;
mod compatibility;
mod duplicates;
mod validate;

use std::collections::BTreeMap;

use crate::{JsonLimits, ProtocolError, ProtocolErrorKind, SchemaDigest};

/// One recursively bounded JSON value with canonical object-key ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedJson {
    value: JsonValue,
    canonical: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl BoundedJson {
    /// Parses and recursively validates JSON within explicit limits.
    ///
    /// # Errors
    ///
    /// Rejects floating-point numbers, duplicate keys, malformed syntax, or exceeded limits.
    pub fn parse(input: &str, limits: JsonLimits) -> Result<Self, ProtocolError> {
        canonical::parse(input, limits)
    }

    /// Creates JSON null.
    #[must_use]
    pub fn null() -> Self {
        canonical::finish(JsonValue::Null)
    }

    /// Creates a JSON boolean.
    #[must_use]
    pub fn boolean(value: bool) -> Self {
        canonical::finish(JsonValue::Bool(value))
    }

    /// Creates a JSON integer.
    #[must_use]
    pub fn integer(value: i64) -> Self {
        canonical::finish(JsonValue::Integer(value))
    }

    /// Creates a bounded JSON string.
    ///
    /// # Errors
    ///
    /// Rejects a string larger than the supplied single-string bound.
    pub fn string(value: String, limits: JsonLimits) -> Result<Self, ProtocolError> {
        if value.len() > limits.max_string_bytes {
            return Err(ProtocolError::at(
                ProtocolErrorKind::JsonLimit,
                "$",
                "JSON string exceeds its byte bound",
            ));
        }
        canonical::finish_checked(JsonValue::String(value), limits)
    }

    /// Creates a bounded JSON array.
    ///
    /// # Errors
    ///
    /// Rejects a recursively over-limit value.
    pub fn array(values: Vec<Self>, limits: JsonLimits) -> Result<Self, ProtocolError> {
        canonical::from_bounded_array(values, limits)
    }

    /// Creates a bounded canonical JSON object.
    ///
    /// # Errors
    ///
    /// Rejects duplicate/noncanonical keys or a recursively over-limit value.
    pub fn object(members: Vec<(String, Self)>, limits: JsonLimits) -> Result<Self, ProtocolError> {
        canonical::from_bounded_object(members, limits)
    }

    /// Borrows canonical compact JSON bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Hashes the exact canonical JSON bytes.
    #[must_use]
    pub fn digest(&self) -> peritus_types::Sha256Digest {
        peritus_codec::sha256(&self.canonical)
    }

    /// Returns an object property when this value is an object.
    #[must_use]
    pub fn property(&self, name: &str) -> Option<Self> {
        match &self.value {
            JsonValue::Object(values) => values.get(name).cloned().map(canonical::finish),
            _ => None,
        }
    }

    /// Returns this value as a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            JsonValue::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns this value as an integer.
    #[must_use]
    pub const fn as_i64(&self) -> Option<i64> {
        match &self.value {
            JsonValue::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns this value as a boolean.
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match &self.value {
            JsonValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns array elements when this value is an array.
    #[must_use]
    pub fn elements(&self) -> Option<Vec<Self>> {
        match &self.value {
            JsonValue::Array(values) => {
                Some(values.iter().cloned().map(canonical::finish).collect())
            }
            _ => None,
        }
    }
}

/// One validated object property schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaProperty {
    name: String,
    schema: Schema,
    required: bool,
}

impl SchemaProperty {
    /// Creates a named property.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-containing names.
    pub fn new(name: String, schema: Schema, required: bool) -> Result<Self, ProtocolError> {
        validate::property_name(&name)?;
        Ok(Self { name, schema, required })
    }

    /// Borrows the canonical property name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Borrows the property's schema.
    #[must_use]
    pub const fn schema(&self) -> &Schema {
        &self.schema
    }
    /// Returns whether the property is required.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }
}

/// Validated immutable schema in C4's deliberately bounded subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schema {
    kind: SchemaKind,
    enum_values: Vec<BoundedJson>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SchemaKind {
    Null,
    Boolean,
    Integer { minimum: Option<i64>, maximum: Option<i64> },
    String { min_bytes: u32, max_bytes: u32 },
    Array { items: Box<Schema>, min_items: u32, max_items: u32 },
    Object { properties: Vec<SchemaProperty>, additional_properties: bool },
}

impl Schema {
    /// Creates a null schema.
    #[must_use]
    pub const fn null() -> Self {
        Self { kind: SchemaKind::Null, enum_values: Vec::new() }
    }
    /// Creates a boolean schema.
    #[must_use]
    pub const fn boolean() -> Self {
        Self { kind: SchemaKind::Boolean, enum_values: Vec::new() }
    }

    /// Creates an integer range schema.
    ///
    /// # Errors
    ///
    /// Rejects an inverted range.
    pub fn integer(minimum: Option<i64>, maximum: Option<i64>) -> Result<Self, ProtocolError> {
        if matches!((minimum, maximum), (Some(min), Some(max)) if min > max) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidSchema,
                "$",
                "integer minimum exceeds maximum",
            ));
        }
        Ok(Self { kind: SchemaKind::Integer { minimum, maximum }, enum_values: Vec::new() })
    }

    /// Creates a UTF-8 byte-cardinality string schema.
    ///
    /// # Errors
    ///
    /// Rejects an inverted byte-cardinality range.
    pub fn string(min_bytes: u32, max_bytes: u32) -> Result<Self, ProtocolError> {
        validate::cardinality(min_bytes, max_bytes, "string")?;
        Ok(Self { kind: SchemaKind::String { min_bytes, max_bytes }, enum_values: Vec::new() })
    }

    /// Creates an array schema.
    ///
    /// # Errors
    ///
    /// Rejects an inverted item-cardinality range.
    pub fn array(items: Self, min_items: u32, max_items: u32) -> Result<Self, ProtocolError> {
        validate::cardinality(min_items, max_items, "array")?;
        Ok(Self {
            kind: SchemaKind::Array { items: Box::new(items), min_items, max_items },
            enum_values: Vec::new(),
        })
    }

    /// Creates an object schema from strictly canonical property order.
    ///
    /// # Errors
    ///
    /// Rejects invalid, duplicate, unsorted, or excessive properties.
    pub fn object(
        properties: Vec<SchemaProperty>,
        additional_properties: bool,
    ) -> Result<Self, ProtocolError> {
        validate::property_order(&properties)?;
        Ok(Self {
            kind: SchemaKind::Object { properties, additional_properties },
            enum_values: Vec::new(),
        })
    }

    /// Restricts a schema to a nonempty, canonical, duplicate-free enumeration.
    ///
    /// # Errors
    ///
    /// Rejects empty, excessive, duplicate, unsorted, or type-invalid values.
    pub fn with_enum(mut self, values: Vec<BoundedJson>) -> Result<Self, ProtocolError> {
        validate::enum_values(&self, &values)?;
        self.enum_values = values;
        Ok(self)
    }

    /// Validates a complete bounded JSON value.
    ///
    /// # Errors
    ///
    /// Rejects any type, cardinality, required-property, or enumeration violation.
    pub fn validate(&self, value: &BoundedJson) -> Result<(), ProtocolError> {
        validate::value(self, &value.value, "$", 1)
    }

    /// Returns canonical compact JSON Schema bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical::schema_bytes(self)
    }

    /// Returns the digest of canonical schema bytes.
    #[must_use]
    pub fn digest(&self) -> SchemaDigest {
        SchemaDigest::new(peritus_codec::sha256(&self.canonical_bytes()))
    }

    /// Classifies compatibility with a candidate successor schema.
    #[must_use]
    pub fn compatibility_with(&self, successor: &Self) -> SchemaCompatibility {
        compatibility::classify(self, successor)
    }
}

/// Compatibility classification within one protocol/tool major version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaCompatibility {
    /// Canonical schemas are identical.
    Equal,
    /// The successor only adds optional object properties.
    Additive,
    /// The successor removes/reinterprets fields or otherwise changes accepted meaning.
    Breaking,
}
