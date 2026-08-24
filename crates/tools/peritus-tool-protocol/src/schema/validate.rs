//! Complete recursive schema and cardinality validation.

use core::cmp::Ordering;

use super::{BoundedJson, JsonValue, Schema, SchemaKind, SchemaProperty};
use crate::{ProtocolError, ProtocolErrorKind};

const MAX_PROPERTIES: usize = 256;
const MAX_PROPERTY_BYTES: usize = 256;
const MAX_SCHEMA_DEPTH: usize = 32;

pub(super) fn property_name(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > MAX_PROPERTY_BYTES
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema property name is invalid or over limit",
        ));
    }
    Ok(())
}

pub(super) fn cardinality(minimum: u32, maximum: u32, path: &str) -> Result<(), ProtocolError> {
    if minimum > maximum {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            path,
            "minimum cardinality exceeds maximum",
        ));
    }
    Ok(())
}

pub(super) fn property_order(properties: &[SchemaProperty]) -> Result<(), ProtocolError> {
    if properties.len() > MAX_PROPERTIES {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema property count exceeds its bound",
        ));
    }
    for property in properties {
        property_name(&property.name)?;
    }
    if properties.windows(2).any(|pair| pair[0].name.as_bytes() >= pair[1].name.as_bytes()) {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema properties are not strictly ordered by UTF-8 bytes",
        ));
    }
    Ok(())
}

pub(super) fn enum_values(schema: &Schema, values: &[BoundedJson]) -> Result<(), ProtocolError> {
    if values.is_empty() || values.len() > MAX_PROPERTIES {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema enum is empty or exceeds its bound",
        ));
    }
    let mut previous: Option<&[u8]> = None;
    for value in values {
        value_without_enum(schema, &value.value, "$", 1)?;
        if previous.is_some_and(|bytes| bytes.cmp(value.canonical_bytes()) != Ordering::Less) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidSchema,
                "$",
                "schema enum values are not strictly canonical",
            ));
        }
        previous = Some(value.canonical_bytes());
    }
    Ok(())
}

pub(super) fn value(
    schema: &Schema,
    value: &JsonValue,
    path: &str,
    depth: usize,
) -> Result<(), ProtocolError> {
    value_without_enum(schema, value, path, depth)?;
    if !schema.enum_values.is_empty()
        && !schema.enum_values.iter().any(|allowed| allowed.value == *value)
    {
        return Err(violation(path, "value is outside the schema enumeration"));
    }
    Ok(())
}

fn value_without_enum(
    schema: &Schema,
    value: &JsonValue,
    path: &str,
    depth: usize,
) -> Result<(), ProtocolError> {
    if depth > MAX_SCHEMA_DEPTH {
        return Err(violation(path, "schema validation depth exceeds its bound"));
    }
    match (&schema.kind, value) {
        (SchemaKind::Null, JsonValue::Null) | (SchemaKind::Boolean, JsonValue::Bool(_)) => Ok(()),
        (SchemaKind::Integer { minimum, maximum }, JsonValue::Integer(value)) => {
            if minimum.is_some_and(|minimum| *value < minimum)
                || maximum.is_some_and(|maximum| *value > maximum)
            {
                Err(violation(path, "integer is outside the allowed range"))
            } else {
                Ok(())
            }
        }
        (SchemaKind::String { min_bytes, max_bytes }, JsonValue::String(value)) => {
            let length = u32::try_from(value.len()).unwrap_or(u32::MAX);
            if length < *min_bytes || length > *max_bytes {
                Err(violation(path, "string is outside the allowed byte cardinality"))
            } else {
                Ok(())
            }
        }
        (SchemaKind::Array { items, min_items, max_items }, JsonValue::Array(values)) => {
            let length = u32::try_from(values.len()).unwrap_or(u32::MAX);
            if length < *min_items || length > *max_items {
                return Err(violation(path, "array is outside the allowed cardinality"));
            }
            for (index, value) in values.iter().enumerate() {
                self::value(items, value, &format!("{path}/{index}"), depth + 1)?;
            }
            Ok(())
        }
        (SchemaKind::Object { properties, additional_properties }, JsonValue::Object(values)) => {
            for property in properties.iter().filter(|property| property.required) {
                if !values.contains_key(&property.name) {
                    return Err(violation(path, "required object property is absent"));
                }
            }
            for (name, value) in values {
                match properties.binary_search_by(|property| property.name.as_str().cmp(name)) {
                    Ok(index) => self::value(
                        &properties[index].schema,
                        value,
                        &format!("{path}/{}", escape_pointer(name)),
                        depth + 1,
                    )?,
                    Err(_) if !additional_properties => {
                        let child_path = format!("{path}/{}", escape_pointer(name));
                        return Err(violation(&child_path, "additional object property is denied"));
                    }
                    Err(_) => {}
                }
            }
            Ok(())
        }
        _ => Err(violation(path, "JSON value has the wrong schema type")),
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn violation(path: &str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::SchemaViolation, path, detail)
}
