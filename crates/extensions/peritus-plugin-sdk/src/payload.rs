//! Recursively bounded canonical JSON payloads.

use serde::Deserialize;
use serde_json::Value;

use crate::{SdkError, SdkErrorKind, canonical};

/// Explicit recursive JSON bounds used by plugin requests and results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonBounds {
    /// Maximum canonical encoded bytes.
    pub max_bytes: usize,
    /// Maximum object/array nesting depth.
    pub max_depth: usize,
    /// Maximum total object members and array elements.
    pub max_members: usize,
    /// Maximum UTF-8 bytes in one string or object key.
    pub max_string_bytes: usize,
}

impl JsonBounds {
    /// Production plugin payload bounds.
    pub const PRODUCTION: Self = Self {
        max_bytes: 1024 * 1024,
        max_depth: 32,
        max_members: 16_384,
        max_string_bytes: 256 * 1024,
    };
}

/// JSON value validated against recursive limits and canonicalized for hashing.
#[derive(Clone, Debug, PartialEq)]
pub struct JsonPayload {
    value: Value,
    canonical: Vec<u8>,
}

impl JsonPayload {
    /// Validates a JSON value against explicit bounds.
    ///
    /// # Errors
    ///
    /// Rejects floating-point numbers, excessive nesting/members/text, or encoded size.
    pub fn new(value: Value, bounds: JsonBounds) -> Result<Self, SdkError> {
        validate(&value, bounds, 1, &mut 0)?;
        let canonical = canonical::bytes(&value)?;
        if canonical.len() > bounds.max_bytes {
            return Err(limit("canonical payload exceeds its byte bound"));
        }
        Ok(Self { value, canonical })
    }

    /// Parses and validates JSON text.
    ///
    /// # Errors
    ///
    /// Rejects malformed JSON or a value outside the supplied bounds.
    pub fn parse(input: &[u8], bounds: JsonBounds) -> Result<Self, SdkError> {
        if input.len() > bounds.max_bytes {
            return Err(limit("input payload exceeds its byte bound"));
        }
        let value = serde_json::from_slice(input).map_err(|error| {
            SdkError::new(SdkErrorKind::InvalidJson, "parse plugin JSON", error.to_string())
        })?;
        Self::new(value, bounds)
    }

    /// Borrows the validated JSON value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Borrows deterministic canonical JSON bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Consumes the wrapper and returns the JSON value.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }
}

impl Eq for JsonPayload {}

impl serde::Serialize for JsonPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::new(value, JsonBounds::PRODUCTION).map_err(serde::de::Error::custom)
    }
}

fn validate(
    value: &Value,
    bounds: JsonBounds,
    depth: usize,
    members: &mut usize,
) -> Result<(), SdkError> {
    if depth > bounds.max_depth {
        return Err(limit("payload nesting exceeds its depth bound"));
    }
    match value {
        Value::Null | Value::Bool(_) => Ok(()),
        Value::Number(number) if number.is_i64() || number.is_u64() => Ok(()),
        Value::Number(_) => Err(SdkError::new(
            SdkErrorKind::InvalidJson,
            "validate plugin JSON",
            "floating-point values are not supported",
        )),
        Value::String(text) => validate_text(text, bounds),
        Value::Array(values) => {
            account(values.len(), bounds, members)?;
            for item in values {
                validate(item, bounds, depth + 1, members)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            account(values.len(), bounds, members)?;
            for (key, item) in values {
                validate_text(key, bounds)?;
                validate(item, bounds, depth + 1, members)?;
            }
            Ok(())
        }
    }
}

fn account(count: usize, bounds: JsonBounds, members: &mut usize) -> Result<(), SdkError> {
    *members =
        members.checked_add(count).ok_or_else(|| limit("payload member count overflowed"))?;
    if *members > bounds.max_members {
        Err(limit("payload member count exceeds its bound"))
    } else {
        Ok(())
    }
}

fn validate_text(text: &str, bounds: JsonBounds) -> Result<(), SdkError> {
    if text.len() > bounds.max_string_bytes {
        Err(limit("payload text exceeds its byte bound"))
    } else {
        Ok(())
    }
}

fn limit(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::LimitExceeded, "validate plugin JSON", detail)
}
