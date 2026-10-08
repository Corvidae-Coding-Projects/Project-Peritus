//! Explicitly bounded canonical JSON payloads and wire policies.

use std::{cell::Cell, collections::BTreeSet, fmt};

use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeOwned, DeserializeSeed as _, MapAccess, SeqAccess, Visitor},
    ser::{SerializeStruct as _, Serializer},
};
use serde_json::Value;

use crate::{SdkError, SdkErrorKind, canonical};

const MAXIMUM_RECURSION_DEPTH: u32 = 128;
const ARBITRARY_NUMBER_TOKEN: &str = "$serde_json::private::Number";
const LIMIT_MARKER: &str = "plugin JSON limit: ";

thread_local! {
    static ACTIVE_BOUNDS: Cell<Option<JsonBounds>> = const { Cell::new(None) };
}

/// Explicit recursive representation policy shared by payload and frame parsing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JsonStructure {
    /// Maximum object/array nesting depth.
    pub max_depth: u32,
    /// Maximum total object members and array elements.
    pub max_members: u32,
    /// Maximum decoded UTF-8 bytes in one string or object key.
    pub max_string_bytes: u32,
}

impl JsonStructure {
    /// Historical protocol-v1 recursive limits.
    pub const V1_COMPATIBILITY: Self = Self {
        max_depth: 32,
        max_members: 16_384,
        max_string_bytes: 256 * 1024,
    };

    /// Validates explicit recursive representation capacity.
    ///
    /// # Errors
    ///
    /// Rejects zero limits or a depth beyond the parser's physical recursion capacity.
    pub fn validate(self) -> Result<Self, SdkError> {
        if self.max_depth == 0
            || self.max_members == 0
            || self.max_string_bytes == 0
            || self.max_depth > MAXIMUM_RECURSION_DEPTH
        {
            Err(limit("JSON structure policy is zero or exceeds parser capacity"))
        } else {
            Ok(self)
        }
    }

    /// Intersects a selected structure policy with an explicit ceiling.
    #[must_use]
    pub const fn narrow(self, ceiling: Self) -> Self {
        Self {
            max_depth: min_u32(self.max_depth, ceiling.max_depth),
            max_members: min_u32(self.max_members, ceiling.max_members),
            max_string_bytes: min_u32(self.max_string_bytes, ceiling.max_string_bytes),
        }
    }
}

impl serde::Serialize for JsonStructure {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("JsonStructure", 3)?;
        state.serialize_field("max_depth", &self.max_depth)?;
        state.serialize_field("max_members", &self.max_members)?;
        state.serialize_field("max_string_bytes", &self.max_string_bytes)?;
        state.end()
    }
}

/// Explicit canonical payload bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonBounds {
    /// Maximum canonical encoded bytes.
    pub max_bytes: u32,
    /// Recursive representation limits.
    pub structure: JsonStructure,
}

impl JsonBounds {
    /// Creates explicit payload bounds.
    #[must_use]
    pub const fn new(max_bytes: u32, structure: JsonStructure) -> Self {
        Self { max_bytes, structure }
    }

    fn validate(self) -> Result<Self, SdkError> {
        if self.max_bytes == 0 {
            return Err(limit("JSON payload byte capacity must be positive"));
        }
        self.structure.validate()?;
        Ok(self)
    }
}

/// One policy applied symmetrically by a frame writer and reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonWirePolicy {
    frame_bytes: u32,
    payload: JsonBounds,
}

impl JsonWirePolicy {
    /// Creates a frame policy from physical frame/payload capacity and recursive limits.
    ///
    /// # Errors
    ///
    /// Rejects zero or inconsistent capacity and invalid recursive limits.
    pub fn new(
        frame_bytes: u32,
        payload_bytes: u32,
        structure: JsonStructure,
    ) -> Result<Self, SdkError> {
        let payload = JsonBounds::new(payload_bytes, structure).validate()?;
        if frame_bytes == 0 || payload_bytes > frame_bytes {
            return Err(limit("JSON payload capacity must fit within its frame capacity"));
        }
        Ok(Self { frame_bytes, payload })
    }

    /// Returns the maximum encoded frame body bytes.
    #[must_use]
    pub const fn frame_bytes(self) -> u32 {
        self.frame_bytes
    }

    /// Returns the exact payload bounds carried by this policy.
    #[must_use]
    pub const fn payload_bounds(self) -> JsonBounds {
        self.payload
    }

    pub(crate) const fn frame_bounds(self) -> JsonBounds {
        JsonBounds::new(
            self.frame_bytes,
            JsonStructure {
                max_depth: MAXIMUM_RECURSION_DEPTH,
                max_members: self.frame_bytes,
                max_string_bytes: self.frame_bytes,
            },
        )
    }
}

/// JSON value validated against explicit bounds and canonicalized for hashing.
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
    /// Rejects excessive nesting, members, text, or canonical encoded size.
    pub fn new(value: Value, bounds: JsonBounds) -> Result<Self, SdkError> {
        bounds.validate()?;
        validate_value(&value, bounds, 1, &mut 0)?;
        let canonical = canonical::value_bytes(&value, bounds.max_bytes as usize)?;
        Ok(Self { value, canonical })
    }

    /// Parses JSON with duplicate rejection before allocating its value tree.
    ///
    /// # Errors
    ///
    /// Rejects malformed or duplicate-bearing JSON and values outside the supplied bounds.
    pub fn parse(input: &[u8], bounds: JsonBounds) -> Result<Self, SdkError> {
        validate_document(input, bounds)?;
        let value = serde_json::from_slice(input)
            .map_err(|error| invalid_json(error.to_string()))?;
        Self::new(value, bounds)
    }

    pub(crate) fn parse_active(input: &[u8]) -> Result<Self, SdkError> {
        let bounds = ACTIVE_BOUNDS
            .with(Cell::get)
            .ok_or_else(|| invalid_json("plugin payload was decoded without a wire policy"))?;
        Self::parse(input, bounds)
    }

    pub(crate) fn validate_bounds(&self, bounds: JsonBounds) -> Result<(), SdkError> {
        bounds.validate()?;
        validate_value(&self.value, bounds, 1, &mut 0)?;
        if self.canonical.len() > bounds.max_bytes as usize {
            return Err(limit("canonical payload exceeds its byte capacity"));
        }
        Ok(())
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

pub(crate) fn deserialize_with_bounds<T: DeserializeOwned>(
    input: &[u8],
    bounds: JsonBounds,
) -> Result<T, serde_json::Error> {
    ACTIVE_BOUNDS.with(|active| {
        let previous = active.replace(Some(bounds));
        let guard = BoundsGuard { active, previous };
        let result = serde_json::from_slice(input);
        drop(guard);
        result
    })
}

struct BoundsGuard<'a> {
    active: &'a Cell<Option<JsonBounds>>,
    previous: Option<JsonBounds>,
}

impl Drop for BoundsGuard<'_> {
    fn drop(&mut self) {
        self.active.set(self.previous);
    }
}

pub(crate) fn validate_document(input: &[u8], bounds: JsonBounds) -> Result<(), SdkError> {
    bounds.validate()?;
    if input.len() > bounds.max_bytes as usize {
        return Err(limit("encoded JSON exceeds its byte capacity"));
    }
    let state = ValidationState { bounds, members: Cell::new(0) };
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    DocumentSeed { state: &state, depth: 1 }
        .deserialize(&mut deserializer)
        .and_then(|()| deserializer.end())
        .map_err(classify_validation_error)
}

struct ValidationState {
    bounds: JsonBounds,
    members: Cell<u64>,
}

impl ValidationState {
    fn check_depth<E: de::Error>(&self, depth: u32) -> Result<(), E> {
        if depth > self.bounds.structure.max_depth {
            Err(E::custom(format!("{LIMIT_MARKER}nesting exceeds its depth capacity")))
        } else {
            Ok(())
        }
    }

    fn check_text<E: de::Error>(&self, text: &str) -> Result<(), E> {
        if text.len() > self.bounds.structure.max_string_bytes as usize {
            Err(E::custom(format!("{LIMIT_MARKER}text exceeds its byte capacity")))
        } else {
            Ok(())
        }
    }

    fn account<E: de::Error>(&self) -> Result<(), E> {
        let next = self
            .members
            .get()
            .checked_add(1)
            .ok_or_else(|| E::custom(format!("{LIMIT_MARKER}member count overflowed")))?;
        if next > u64::from(self.bounds.structure.max_members) {
            return Err(E::custom(format!("{LIMIT_MARKER}member count exceeds its capacity")));
        }
        self.members.set(next);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct DocumentSeed<'a> {
    state: &'a ValidationState,
    depth: u32,
}

impl<'de> de::DeserializeSeed<'de> for DocumentSeed<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.state.check_depth(self.depth)?;
        deserializer.deserialize_any(DocumentVisitor(self))
    }
}

struct DocumentVisitor<'a>(DocumentSeed<'a>);

impl<'de> Visitor<'de> for DocumentVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON within the selected wire policy")
    }

    fn visit_bool<E: de::Error>(self, _value: bool) -> Result<(), E> { Ok(()) }
    fn visit_i64<E: de::Error>(self, _value: i64) -> Result<(), E> { Ok(()) }
    fn visit_i128<E: de::Error>(self, _value: i128) -> Result<(), E> { Ok(()) }
    fn visit_u64<E: de::Error>(self, _value: u64) -> Result<(), E> { Ok(()) }
    fn visit_u128<E: de::Error>(self, _value: u128) -> Result<(), E> { Ok(()) }
    fn visit_f64<E: de::Error>(self, _value: f64) -> Result<(), E> { Ok(()) }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        self.0.state.check_text(value)
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<(), E> {
        self.visit_str(&value)
    }

    fn visit_none<E: de::Error>(self) -> Result<(), E> { Ok(()) }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> { Ok(()) }

    fn visit_some<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.0.deserialize(deserializer)
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        self.0.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        let child = DocumentSeed {
            state: self.0.state,
            depth: self.0.depth.saturating_add(1),
        };
        while sequence.next_element_seed(child)?.is_some() {
            self.0.state.account()?;
        }
        Ok(())
    }

    fn visit_map<A>(self, mut object: A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        let Some(first_key) = object.next_key::<String>()? else {
            return Ok(());
        };
        if first_key == ARBITRARY_NUMBER_TOKEN {
            let _: String = object.next_value()?;
            if object.next_key::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("malformed exact JSON number"));
            }
            return Ok(());
        }

        let child = DocumentSeed {
            state: self.0.state,
            depth: self.0.depth.saturating_add(1),
        };
        let mut keys = BTreeSet::new();
        self.0.state.check_text(&first_key)?;
        self.0.state.account()?;
        keys.insert(first_key);
        object.next_value_seed(child)?;
        while let Some(key) = object.next_key::<String>()? {
            self.0.state.check_text(&key)?;
            self.0.state.account()?;
            if !keys.insert(key) {
                return Err(de::Error::custom("duplicate JSON object member"));
            }
            object.next_value_seed(child)?;
        }
        Ok(())
    }
}

fn validate_value(
    value: &Value,
    bounds: JsonBounds,
    depth: u32,
    members: &mut u64,
) -> Result<(), SdkError> {
    if depth > bounds.structure.max_depth {
        return Err(limit("payload nesting exceeds its depth capacity"));
    }
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(text) => validate_text(text, bounds),
        Value::Array(values) => {
            account(values.len(), bounds, members)?;
            for item in values {
                validate_value(item, bounds, depth.saturating_add(1), members)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            account(values.len(), bounds, members)?;
            for (key, item) in values {
                validate_text(key, bounds)?;
                validate_value(item, bounds, depth.saturating_add(1), members)?;
            }
            Ok(())
        }
    }
}

fn account(count: usize, bounds: JsonBounds, members: &mut u64) -> Result<(), SdkError> {
    let count = u64::try_from(count).map_err(|_| limit("payload member count overflowed"))?;
    *members = members
        .checked_add(count)
        .ok_or_else(|| limit("payload member count overflowed"))?;
    if *members > u64::from(bounds.structure.max_members) {
        Err(limit("payload member count exceeds its capacity"))
    } else {
        Ok(())
    }
}

fn validate_text(text: &str, bounds: JsonBounds) -> Result<(), SdkError> {
    if text.len() > bounds.structure.max_string_bytes as usize {
        Err(limit("payload text exceeds its byte capacity"))
    } else {
        Ok(())
    }
}

fn classify_validation_error(error: serde_json::Error) -> SdkError {
    let detail = error.to_string();
    if detail.contains(LIMIT_MARKER) {
        SdkError::new(SdkErrorKind::LimitExceeded, "validate plugin JSON", detail)
    } else {
        invalid_json(detail)
    }
}

fn invalid_json(detail: impl Into<String>) -> SdkError {
    SdkError::new(SdkErrorKind::InvalidJson, "parse plugin JSON", detail)
}

fn limit(detail: &'static str) -> SdkError {
    SdkError::new(SdkErrorKind::LimitExceeded, "validate plugin JSON", detail)
}

const fn min_u32(left: u32, right: u32) -> u32 {
    if left < right { left } else { right }
}
