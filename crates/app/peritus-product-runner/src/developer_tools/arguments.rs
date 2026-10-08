//! Admission for the closed JSON Schema subset used by the host's workspace tool catalog.
//! This is not a general JSON Schema implementation. Unsupported catalog keywords fail closed;
//! permissions, grounding, filesystem checks and effect receipts remain executor responsibilities.

use std::{collections::BTreeMap, fmt};

use peritus_agent::DeveloperLoopError;
use serde_json::{Map, Value};

use super::{argument_contract, catalog, path::tool};

type Catalog = BTreeMap<String, Value>;
const KEYWORDS: &[&str] = &[
    "$comment",
    "$id",
    "$schema",
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "prefixItems",
    "contains",
    "minContains",
    "maxContains",
    "enum",
    "const",
    "default",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minLength",
    "maxLength",
    "minProperties",
    "maxProperties",
    "dependentRequired",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "title",
    "description",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];
const TYPES: &[&str] = &["object", "array", "string", "integer", "number", "boolean", "null"];

enum ArgumentFailure {
    Malformed(String),
    Capacity(String),
}

impl fmt::Display for ArgumentFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(detail) | Self::Capacity(detail) => formatter.write_str(detail),
        }
    }
}

pub(super) fn validate(name: &str, arguments: &Value) -> Result<(), DeveloperLoopError> {
    // Catalog construction can depend on temporarily unavailable provider/protocol capacity.
    // Cache no failure: a later call must be able to recover after that dependency is repaired.
    let catalog = load().map_err(|detail| {
        tool(format!(
            "Host tool catalog is temporarily unavailable: {detail}. Retry after its schema dependencies are repaired."
        ))
    })?;
    let schema =
        catalog.get(name).ok_or_else(|| tool("model requested an undeclared developer tool"))?;
    validate_value(schema, arguments, "$", 0).map_err(|failure| match failure {
        ArgumentFailure::Malformed(detail) => tool(format!(
            "Invalid tool arguments: {detail}. No action was executed. Correct the fields to match the declared tool schema; do not repeat the unchanged call."
        )),
        ArgumentFailure::Capacity(detail) => tool(format!(
            "Tool argument representation exceeded temporary host capacity: {detail}. No action was executed. Retry with a smaller physical page or chunk without reducing the logical work."
        )),
    })
}

fn load() -> Result<Catalog, String> {
    let mut definitions = catalog::definitions().map_err(|error| error.to_string())?;
    definitions.push(catalog::in_place_definition().map_err(|error| error.to_string())?);
    definitions
        .into_iter()
        .map(|definition| {
            let schema: Value = serde_json::from_slice(definition.parameters().canonical_bytes())
                .map_err(|error| error.to_string())?;
            check_schema(&schema, 0)?;
            Ok((definition.name().as_str().to_owned(), schema))
        })
        .collect()
}

fn check_schema(schema: &Value, depth: usize) -> Result<(), String> {
    if schema.is_boolean() {
        return Ok(());
    }
    let object = schema.as_object().ok_or("schema must be an object")?;
    if depth > argument_contract::MAX_SCHEMA_DEPTH {
        return Err("host catalog schema depth exceeds the advertised representation".to_owned());
    }
    if object.keys().any(|key| !KEYWORDS.contains(&key.as_str())) {
        return Err("unsupported host catalog schema".to_owned());
    }
    if let Some(kind) = object.get("type") {
        let valid = kind.as_str().is_some_and(|kind| TYPES.contains(&kind))
            || kind.as_array().is_some_and(|kinds| {
                !kinds.is_empty()
                    && kinds
                        .iter()
                        .all(|kind| kind.as_str().is_some_and(|kind| TYPES.contains(&kind)))
            });
        if !valid {
            return Err("unsupported host tool value type".to_owned());
        }
    }
    if let Some(properties) = object.get("properties") {
        for child in properties.as_object().ok_or("invalid schema properties")?.values() {
            check_schema(child, depth + 1)?;
        }
    }
    if let Some(required) = object.get("required") {
        if !required
            .as_array()
            .is_some_and(|names| names.iter().all(Value::is_string))
        {
            return Err("invalid required fields".to_owned());
        }
    }
    if let Some(additional) = object.get("additionalProperties")
        && !additional.is_boolean()
    {
        check_schema(additional, depth + 1)?;
    }
    for keyword in ["items", "contains", "not"] {
        if let Some(child) = object.get(keyword) {
            check_schema(child, depth + 1)?;
        }
    }
    if let Some(prefix) = object.get("prefixItems") {
        for child in prefix.as_array().ok_or("invalid prefixItems")? {
            check_schema(child, depth + 1)?;
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = object.get(keyword) {
            let children = children.as_array().filter(|children| !children.is_empty())
                .ok_or("invalid schema alternatives")?;
            for child in children {
                check_schema(child, depth + 1)?;
            }
        }
    }
    if let Some(choices) = object.get("enum")
        && !choices.as_array().is_some_and(|choices| !choices.is_empty())
    {
        return Err("schema enum is empty or invalid".to_owned());
    }
    if let Some(dependencies) = object.get("dependentRequired") {
        for required in dependencies.as_object().ok_or("invalid dependentRequired")?.values() {
            if !required
                .as_array()
                .is_some_and(|names| names.iter().all(Value::is_string))
            {
                return Err("invalid dependentRequired fields".to_owned());
            }
        }
    }
    Ok(())
}

fn validate_value(
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
) -> Result<(), ArgumentFailure> {
    if depth > argument_contract::MAX_SCHEMA_DEPTH {
        return Err(ArgumentFailure::Capacity("argument depth exceeded".to_owned()));
    }
    if let Some(accepts) = schema.as_bool() {
        return if accepts {
            Ok(())
        } else {
            Err(malformed(format!("{path} is denied by the declared schema")))
        };
    }
    let schema = schema
        .as_object()
        .ok_or_else(|| malformed("host schema is not an object"))?;
    if let Some(kinds) = schema.get("type")
        && !schema_type_matches(kinds, value)
    {
        return Err(malformed(format!("{path} has the wrong declared type")));
    }
    if schema.get("const").is_some_and(|constant| constant != value) {
        return Err(malformed(format!("{path} differs from its declared constant")));
    }
    if schema.get("enum").and_then(Value::as_array).is_some_and(|choices| !choices.contains(value))
    {
        return Err(malformed(format!("{path} is not a declared enum value")));
    }
    if let Some(children) = schema.get("allOf").and_then(Value::as_array) {
        for child in children {
            validate_value(child, value, path, depth + 1)?;
        }
    }
    for (keyword, exact) in [("anyOf", false), ("oneOf", true)] {
        if let Some(children) = schema.get(keyword).and_then(Value::as_array) {
            let mut matches = 0_usize;
            for child in children {
                match validate_value(child, value, path, depth + 1) {
                    Ok(()) => matches += 1,
                    Err(ArgumentFailure::Malformed(_)) => {}
                    Err(capacity @ ArgumentFailure::Capacity(_)) => return Err(capacity),
                }
            }
            if matches == 0 || (exact && matches != 1) {
                return Err(malformed(format!("{path} does not match the declared {keyword}")));
            }
        }
    }
    if let Some(child) = schema.get("not") {
        match validate_value(child, value, path, depth + 1) {
            Ok(()) => return Err(malformed(format!("{path} matches a forbidden schema"))),
            Err(ArgumentFailure::Malformed(_)) => {}
            Err(capacity @ ArgumentFailure::Capacity(_)) => return Err(capacity),
        }
    }
    match value {
        Value::Object(values) => validate_object(schema, values, path, depth)?,
        Value::Array(values) => {
            cardinality(schema, values.len(), "minItems", "maxItems", path)?;
            let prefix = schema.get("prefixItems").and_then(Value::as_array);
            if let Some(prefix) = prefix {
                for (index, (value, child_schema)) in values.iter().zip(prefix).enumerate() {
                    validate_value(child_schema, value, &format!("{path}/{index}"), depth + 1)?;
                }
            }
            if let Some(items) = schema.get("items") {
                let offset = prefix.map_or(0, Vec::len);
                for (index, child) in values.iter().enumerate().skip(offset) {
                    validate_value(items, child, &format!("{path}/{index}"), depth + 1)?;
                }
            }
            if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
                for (index, child) in values.iter().enumerate() {
                    if values[..index].contains(child) {
                        return Err(malformed(format!("{path} contains duplicate array values")));
                    }
                }
            }
            if let Some(contains) = schema.get("contains") {
                let count = values
                    .iter()
                    .filter(|child| validate_value(contains, child, path, depth + 1).is_ok())
                    .count();
                cardinality(schema, count, "minContains", "maxContains", path)?;
                if schema.get("minContains").is_none() && count == 0 {
                    return Err(malformed(format!("{path} contains no declared matching item")));
                }
            }
        }
        Value::String(text) => {
            cardinality(schema, text.chars().count(), "minLength", "maxLength", path)?;
        }
        Value::Number(number) => {
            let number = number
                .as_f64()
                .ok_or_else(|| malformed(format!("{path} is not a represented number")))?;
            if schema.get("minimum").and_then(Value::as_f64).is_some_and(|min| number < min)
                || schema.get("maximum").and_then(Value::as_f64).is_some_and(|max| number > max)
                || schema
                    .get("exclusiveMinimum")
                    .and_then(Value::as_f64)
                    .is_some_and(|min| number <= min)
                || schema
                    .get("exclusiveMaximum")
                    .and_then(Value::as_f64)
                    .is_some_and(|max| number >= max)
            {
                return Err(malformed(format!("{path} is outside the declared numeric range")));
            }
        }
        Value::Null | Value::Bool(_) => {}
    }
    Ok(())
}

fn validate_object(
    schema: &Map<String, Value>,
    values: &Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<(), ArgumentFailure> {
    cardinality(schema, values.len(), "minProperties", "maxProperties", path)?;
    let properties = schema.get("properties").and_then(Value::as_object);
    for required in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
        let name = required.as_str().ok_or_else(|| malformed("invalid host schema"))?;
        if !values.contains_key(name) {
            return Err(malformed(format!("{path}/{name} is required")));
        }
    }
    for (name, value) in values {
        if let Some(child) = properties.and_then(|properties| properties.get(name)) {
            validate_value(child, value, &format!("{path}/{name}"), depth + 1)?;
            continue;
        }
        match schema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                return Err(malformed(format!("{path} contains an undeclared field")));
            }
            Some(child) if !child.is_boolean() => {
                validate_value(child, value, &format!("{path}/{name}"), depth + 1)?;
            }
            Some(Value::Bool(true)) | None => {}
            Some(_) => return Err(malformed("invalid additionalProperties schema")),
        }
    }
    if let Some(dependencies) = schema.get("dependentRequired").and_then(Value::as_object) {
        for (name, required) in dependencies {
            if !values.contains_key(name) {
                continue;
            }
            for dependent in required.as_array().into_iter().flatten() {
                let dependent = dependent
                    .as_str()
                    .ok_or_else(|| malformed("invalid dependentRequired schema"))?;
                if !values.contains_key(dependent) {
                    return Err(malformed(format!("{path}/{dependent} is required by {name}")));
                }
            }
        }
    }
    Ok(())
}

fn cardinality(
    schema: &Map<String, Value>,
    count: usize,
    minimum: &str,
    maximum: &str,
    path: &str,
) -> Result<(), ArgumentFailure> {
    let count = u64::try_from(count)
        .map_err(|_| ArgumentFailure::Capacity("argument length overflow".to_owned()))?;
    if schema.get(minimum).and_then(Value::as_u64).is_some_and(|min| count < min)
        || schema.get(maximum).and_then(Value::as_u64).is_some_and(|max| count > max)
    {
        Err(malformed(format!("{path} is outside the declared length range")))
    } else {
        Ok(())
    }
}

fn schema_type_matches(kinds: &Value, value: &Value) -> bool {
    kinds.as_str().is_some_and(|kind| type_matches(kind, value))
        || kinds
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str().is_some_and(|kind| type_matches(kind, value))))
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn malformed(detail: impl Into<String>) -> ArgumentFailure {
    ArgumentFailure::Malformed(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_catalog_schema_is_supported_and_read_only_definitions_agree() {
        let catalog = load().unwrap();
        for definition in catalog::read_only_definitions().unwrap() {
            let schema: Value =
                serde_json::from_slice(definition.parameters().canonical_bytes()).unwrap();
            assert_eq!(catalog[definition.name().as_str()], schema);
        }
        assert!(check_schema(&json!({"type":"string", "pattern":"unsupported"}), 0).is_err());
    }

    #[test]
    fn invalid_fields_are_rejected_instead_of_defaulted_or_coerced() {
        for (name, arguments) in [
            ("workspace_list", json!({"depth":"3"})),
            ("workspace_write", json!({"path":"a", "contents":"typo"})),
            ("workspace_patch", json!({"path":"a", "old":"a", "new":"b", "replace_all":"false"})),
            ("run_command", json!({"program":"cargo", "args":[5], "purpose":"verification"})),
            ("run_command", json!({"program":"cargo", "args":[], "purpose":"verify"})),
            ("run_command", json!({"program":"cargo", "args":[]})),
            (
                "run_command",
                json!({"program":"cargo", "args":[], "purpose":"verification", "timeout_seconds":0}),
            ),
            ("workspace_scope", json!({"paths":[]})),
        ] {
            assert!(validate(name, &arguments).is_err(), "{name}: {arguments}");
        }
        assert!(validate("workspace_list", &json!({})).is_ok());
        assert!(validate("run_command", &json!({"program":"cargo", "args":[], "purpose":"verification", "timeout_seconds":601})).is_ok());
        assert!(
            validate("workspace_write", &json!({"path":"a", "content":"é".repeat(100_000)}))
                .is_ok(),
            "preserve existing file size limits"
        );
    }
}
