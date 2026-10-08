//! Complete recursive schema and cardinality validation.

use core::cmp::Ordering;

use super::{BoundedJson, JsonValue, Schema, SchemaContract, SchemaKind, SchemaProperty};
use crate::{JsonLimits, ProtocolError, ProtocolErrorKind};

pub(super) fn property_name(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.contains('\0') || value.chars().any(char::is_control) {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema property name is empty or contains a control character",
        ));
    }
    Ok(())
}

pub(super) fn cardinality(minimum: u64, maximum: u64, path: &str) -> Result<(), ProtocolError> {
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
    if values.is_empty() {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "schema enum is empty",
        ));
    }
    let mut previous: Option<&[u8]> = None;
    for value in values {
        value_without_enum(
            schema,
            &value.value,
            "$",
            1,
            JsonLimits::MAXIMUM.max_depth(),
        )?;
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
    max_depth: usize,
) -> Result<(), ProtocolError> {
    value_without_enum(schema, value, path, depth, max_depth)?;
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
    max_depth: usize,
) -> Result<(), ProtocolError> {
    if depth > max_depth {
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
        (SchemaKind::Integer { minimum, maximum }, JsonValue::Unsigned(value)) => {
            let below_minimum = minimum.is_some_and(|minimum| {
                u64::try_from(minimum).is_ok_and(|minimum| *value < minimum)
            });
            let above_maximum = maximum.is_some_and(|maximum| {
                u64::try_from(maximum).map_or(true, |maximum| *value > maximum)
            });
            if below_minimum || above_maximum {
                Err(violation(path, "integer is outside the allowed range"))
            } else {
                Ok(())
            }
        }
        (SchemaKind::String { minimum, maximum }, JsonValue::String(value)) => {
            let observed = match schema.contract {
                SchemaContract::LegacyV1 => value.len(),
                SchemaContract::JsonSchema202012 => value.chars().count(),
            };
            let length = u64::try_from(observed)
                .map_err(|_| violation(path, "string cardinality is not representable"))?;
            if length < *minimum || maximum.is_some_and(|maximum| length > maximum) {
                let detail = match schema.contract {
                    SchemaContract::LegacyV1 => {
                        "string is outside the allowed UTF-8 byte cardinality"
                    }
                    SchemaContract::JsonSchema202012 => {
                        "string is outside the allowed Unicode character cardinality"
                    }
                };
                Err(violation(path, detail))
            } else {
                Ok(())
            }
        }
        (SchemaKind::Array { items, min_items, max_items }, JsonValue::Array(values)) => {
            let length = u64::try_from(values.len())
                .map_err(|_| violation(path, "array cardinality is not representable"))?;
            if length < *min_items || max_items.is_some_and(|maximum| length > maximum) {
                return Err(violation(path, "array is outside the allowed cardinality"));
            }
            for (index, value) in values.iter().enumerate() {
                self::value(
                    items,
                    value,
                    &format!("{path}/{index}"),
                    depth + 1,
                    max_depth,
                )?;
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
                        max_depth,
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

pub(super) fn definition(
    schema: &Schema,
    contract: SchemaContract,
    limits: JsonLimits,
) -> Result<(), ProtocolError> {
    definition_at(schema, contract, limits, 1)
}

fn definition_at(
    schema: &Schema,
    contract: SchemaContract,
    limits: JsonLimits,
    depth: usize,
) -> Result<(), ProtocolError> {
    if depth > limits.max_depth() {
        return Err(invalid(
            "schema depth cannot fit the selected JSON frame",
        ));
    }
    if schema.contract != contract {
        return Err(ProtocolError::at(
            ProtocolErrorKind::InvalidSchema,
            "$",
            "nested schema uses a different versioned contract",
        ));
    }
    for value in &schema.enum_values {
        value.validate_limits(limits)?;
        value_without_enum(schema, &value.value, "$", 1, limits.max_depth())?;
    }
    let maximum_strings = representable(limits.max_string_bytes());
    let maximum_members = representable(limits.max_members());
    match &schema.kind {
        SchemaKind::String { minimum, .. } if *minimum > maximum_strings => Err(
            invalid("minimum string cardinality cannot fit the selected JSON frame"),
        ),
        SchemaKind::Array { items, min_items, .. } => {
            if *min_items > maximum_members {
                return Err(invalid(
                    "minimum array cardinality cannot fit the selected JSON frame",
                ));
            }
            definition_at(items, contract, limits, depth + 1)
        }
        SchemaKind::Object { properties, .. } => {
            let required = properties.iter().filter(|property| property.required).count();
            if required > limits.max_members() {
                return Err(invalid(
                    "required property count cannot fit the selected JSON frame",
                ));
            }
            for property in properties {
                definition_at(&property.schema, contract, limits, depth + 1)?;
            }
            Ok(())
        }
        SchemaKind::Null
        | SchemaKind::Boolean
        | SchemaKind::Integer { .. }
        | SchemaKind::String { .. } => Ok(()),
    }
}

fn representable(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn invalid(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidSchema, "$", detail)
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn violation(path: &str, detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::SchemaViolation, path, detail)
}
