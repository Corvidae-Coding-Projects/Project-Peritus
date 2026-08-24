//! Canonical JSON parsing and rendering.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{BoundedJson, JsonValue, Schema, SchemaKind};
use crate::{JsonLimits, ProtocolError, ProtocolErrorKind};

pub(super) fn parse(input: &str, limits: JsonLimits) -> Result<BoundedJson, ProtocolError> {
    if input.len() > limits.max_bytes {
        return Err(limit("$", "JSON input exceeds its byte bound"));
    }
    let raw: Value = serde_json::from_str(input).map_err(|_| {
        ProtocolError::at(ProtocolErrorKind::InvalidJson, "$", "JSON syntax is malformed")
    })?;
    if super::duplicates::contains(input) {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidJson,
            "$",
            "JSON object contains a duplicate key",
        ));
    }
    let mut members = 0;
    let value = convert(raw, limits, 1, &mut members, "$")?;
    finish_checked(value, limits)
}

fn convert(
    raw: Value,
    limits: JsonLimits,
    depth: usize,
    members: &mut usize,
    path: &str,
) -> Result<JsonValue, ProtocolError> {
    if depth > limits.max_depth {
        return Err(limit(path, "JSON depth exceeds its bound"));
    }
    match raw {
        Value::Null => Ok(JsonValue::Null),
        Value::Bool(value) => Ok(JsonValue::Bool(value)),
        Value::Number(value) => value.as_i64().map(JsonValue::Integer).ok_or_else(|| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidJson,
                path,
                "only signed 64-bit JSON integers are supported",
            )
        }),
        Value::String(value) => {
            check_string(&value, limits, path)?;
            Ok(JsonValue::String(value))
        }
        Value::Array(values) => {
            account(members, values.len(), limits, path)?;
            let mut converted = Vec::with_capacity(values.len());
            for (index, value) in values.into_iter().enumerate() {
                let child_path = format!("{path}/{index}");
                converted.push(convert(value, limits, depth + 1, members, &child_path)?);
            }
            Ok(JsonValue::Array(converted))
        }
        Value::Object(values) => {
            account(members, values.len(), limits, path)?;
            let mut converted = BTreeMap::new();
            for (key, value) in values {
                check_string(&key, limits, path)?;
                let child_path = format!("{path}/{}", escape_pointer(&key));
                converted.insert(key, convert(value, limits, depth + 1, members, &child_path)?);
            }
            Ok(JsonValue::Object(converted))
        }
    }
}

pub(super) fn finish(value: JsonValue) -> BoundedJson {
    let mut canonical = Vec::new();
    write_value(&value, &mut canonical);
    BoundedJson { value, canonical }
}

pub(super) fn finish_checked(
    value: JsonValue,
    limits: JsonLimits,
) -> Result<BoundedJson, ProtocolError> {
    validate_constructed(&value, limits, 1, &mut 0, "$")?;
    let result = finish(value);
    if result.canonical.len() > limits.max_bytes {
        return Err(limit("$", "canonical JSON exceeds its byte bound"));
    }
    Ok(result)
}

pub(super) fn from_bounded_array(
    values: Vec<BoundedJson>,
    limits: JsonLimits,
) -> Result<BoundedJson, ProtocolError> {
    finish_checked(JsonValue::Array(values.into_iter().map(|value| value.value).collect()), limits)
}

pub(super) fn from_bounded_object(
    values: Vec<(String, BoundedJson)>,
    limits: JsonLimits,
) -> Result<BoundedJson, ProtocolError> {
    let mut object = BTreeMap::new();
    for (key, value) in values {
        check_string(&key, limits, "$")?;
        if object.insert(key, value.value).is_some() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidJson,
                "$",
                "JSON object contains a duplicate key",
            ));
        }
    }
    finish_checked(JsonValue::Object(object), limits)
}

pub(super) fn schema_bytes(schema: &Schema) -> Vec<u8> {
    finish(schema_value(schema)).canonical
}

fn schema_value(schema: &Schema) -> JsonValue {
    let mut fields: BTreeMap<String, JsonValue> = BTreeMap::new();
    match &schema.kind {
        SchemaKind::Null => {
            fields.insert("type".to_owned(), JsonValue::String("null".to_owned()));
        }
        SchemaKind::Boolean => {
            fields.insert("type".to_owned(), JsonValue::String("boolean".to_owned()));
        }
        SchemaKind::Integer { minimum, maximum } => {
            fields.insert("type".to_owned(), JsonValue::String("integer".to_owned()));
            if let Some(value) = maximum {
                fields.insert("maximum".to_owned(), JsonValue::Integer(*value));
            }
            if let Some(value) = minimum {
                fields.insert("minimum".to_owned(), JsonValue::Integer(*value));
            }
        }
        SchemaKind::String { min_bytes, max_bytes } => {
            fields.insert("type".to_owned(), JsonValue::String("string".to_owned()));
            fields.insert("maxLength".to_owned(), JsonValue::Integer(i64::from(*max_bytes)));
            fields.insert("minLength".to_owned(), JsonValue::Integer(i64::from(*min_bytes)));
        }
        SchemaKind::Array { items, min_items, max_items } => {
            fields.insert("type".to_owned(), JsonValue::String("array".to_owned()));
            fields.insert("items".to_owned(), schema_value(items));
            fields.insert("maxItems".to_owned(), JsonValue::Integer(i64::from(*max_items)));
            fields.insert("minItems".to_owned(), JsonValue::Integer(i64::from(*min_items)));
        }
        SchemaKind::Object { properties, additional_properties } => {
            fields.insert("type".to_owned(), JsonValue::String("object".to_owned()));
            fields
                .insert("additionalProperties".to_owned(), JsonValue::Bool(*additional_properties));
            let props = properties
                .iter()
                .map(|property| (property.name.clone(), schema_value(&property.schema)))
                .collect();
            fields.insert("properties".to_owned(), JsonValue::Object(props));
            let required = properties
                .iter()
                .filter(|property| property.required)
                .map(|property| JsonValue::String(property.name.clone()))
                .collect();
            fields.insert("required".to_owned(), JsonValue::Array(required));
        }
    }
    if !schema.enum_values.is_empty() {
        let values = schema.enum_values.iter().map(|value| value.value.clone()).collect();
        fields.insert("enum".to_owned(), JsonValue::Array(values));
    }
    JsonValue::Object(fields)
}

fn write_value(value: &JsonValue, output: &mut Vec<u8>) {
    match value {
        JsonValue::Null => output.extend_from_slice(b"null"),
        JsonValue::Bool(false) => output.extend_from_slice(b"false"),
        JsonValue::Bool(true) => output.extend_from_slice(b"true"),
        JsonValue::Integer(value) => output.extend_from_slice(value.to_string().as_bytes()),
        JsonValue::String(value) => {
            write_string(value, output);
        }
        JsonValue::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output);
            }
            output.push(b']');
        }
        JsonValue::Object(values) => {
            output.push(b'{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_string(key, output);
                output.push(b':');
                write_value(value, output);
            }
            output.push(b'}');
        }
    }
}

fn write_string(value: &str, output: &mut Vec<u8>) {
    output.push(b'"');
    for character in value.chars() {
        match character {
            '"' => output.extend_from_slice(br#"\""#),
            '\\' => output.extend_from_slice(br"\\"),
            '\u{08}' => output.extend_from_slice(br"\b"),
            '\u{0c}' => output.extend_from_slice(br"\f"),
            '\n' => output.extend_from_slice(br"\n"),
            '\r' => output.extend_from_slice(br"\r"),
            '\t' => output.extend_from_slice(br"\t"),
            value if value <= '\u{1f}' => {
                output.extend_from_slice(br"\u00");
                let code = u32::from(value);
                output.push(hex(u8::try_from(code >> 4).unwrap_or(0)));
                output.push(hex(u8::try_from(code & 0x0f).unwrap_or(0)));
            }
            value => {
                let mut bytes = [0; 4];
                output.extend_from_slice(value.encode_utf8(&mut bytes).as_bytes());
            }
        }
    }
    output.push(b'"');
}

const fn hex(value: u8) -> u8 {
    if value < 10 { b'0' + value } else { b'a' + (value - 10) }
}

fn validate_constructed(
    value: &JsonValue,
    limits: JsonLimits,
    depth: usize,
    members: &mut usize,
    path: &str,
) -> Result<(), ProtocolError> {
    if depth > limits.max_depth {
        return Err(limit(path, "JSON depth exceeds its bound"));
    }
    match value {
        JsonValue::String(value) => check_string(value, limits, path),
        JsonValue::Array(values) => {
            account(members, values.len(), limits, path)?;
            for (index, value) in values.iter().enumerate() {
                let child_path = format!("{path}/{index}");
                validate_constructed(value, limits, depth + 1, members, &child_path)?;
            }
            Ok(())
        }
        JsonValue::Object(values) => {
            account(members, values.len(), limits, path)?;
            for (key, value) in values {
                check_string(key, limits, path)?;
                let child_path = format!("{path}/{}", escape_pointer(key));
                validate_constructed(value, limits, depth + 1, members, &child_path)?;
            }
            Ok(())
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Integer(_) => Ok(()),
    }
}

fn account(
    total: &mut usize,
    count: usize,
    limits: JsonLimits,
    path: &str,
) -> Result<(), ProtocolError> {
    *total = total.checked_add(count).ok_or_else(|| limit(path, "JSON member count overflow"))?;
    if *total > limits.max_members {
        return Err(limit(path, "JSON member count exceeds its bound"));
    }
    Ok(())
}

fn check_string(value: &str, limits: JsonLimits, path: &str) -> Result<(), ProtocolError> {
    if value.len() > limits.max_string_bytes {
        return Err(limit(path, "JSON string exceeds its byte bound"));
    }
    Ok(())
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn limit(path: &str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::JsonLimit, path, detail)
}
