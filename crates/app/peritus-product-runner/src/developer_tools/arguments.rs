//! Admission for the closed JSON Schema subset used by the host's workspace tool catalog.
//! This is not a general JSON Schema implementation. Unsupported catalog keywords fail closed;
//! permissions, grounding, filesystem checks and effect receipts remain executor responsibilities.

use std::{collections::BTreeMap, sync::OnceLock};

use peritus_agent::DeveloperLoopError;
use serde_json::{Map, Value};

use super::{catalog, path::tool};

type Catalog = BTreeMap<String, Value>;
static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "default",
    "minimum",
    "maximum",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
];

pub(super) fn validate(name: &str, arguments: &Value) -> Result<(), DeveloperLoopError> {
    let catalog =
        CATALOG.get_or_init(load).as_ref().map_err(|_| tool("host tool catalog is invalid"))?;
    let schema =
        catalog.get(name).ok_or_else(|| tool("model requested an undeclared developer tool"))?;
    validate_value(schema, arguments, "$", 0).map_err(|detail| tool(format!(
        "Invalid tool arguments: {detail}. No action was executed. Correct the fields to match the declared tool schema; do not repeat the unchanged call."
    )))
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
    let object = schema.as_object().ok_or("schema must be an object")?;
    if depth > 16 || object.keys().any(|key| !KEYWORDS.contains(&key.as_str())) {
        return Err("unsupported host catalog schema".to_owned());
    }
    match object.get("type").and_then(Value::as_str) {
        Some("object") => {
            let properties =
                object.get("properties").and_then(Value::as_object).ok_or("missing properties")?;
            if object.get("additionalProperties") != Some(&Value::Bool(false)) {
                return Err("host tool objects must be closed".to_owned());
            }
            if let Some(required) = object.get("required") {
                for name in required.as_array().ok_or("invalid required fields")? {
                    if !properties.contains_key(name.as_str().ok_or("invalid required name")?) {
                        return Err("required field absent from schema".to_owned());
                    }
                }
            }
            for child in properties.values() {
                check_schema(child, depth + 1)?;
            }
        }
        Some("array") => {
            check_schema(object.get("items").ok_or("missing array schema")?, depth + 1)?;
        }
        Some("string" | "integer" | "boolean") => {}
        _ => return Err("unsupported host tool value type".to_owned()),
    }
    Ok(())
}

fn validate_value(schema: &Value, value: &Value, path: &str, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("argument depth exceeded".to_owned());
    }
    match schema["type"].as_str() {
        Some("object") => {
            let values = value.as_object().ok_or_else(|| format!("{path} must be an object"))?;
            let properties = schema["properties"].as_object().ok_or("invalid host schema")?;
            validate_object(schema, properties, values, path, depth)?;
        }
        Some("array") => {
            let values = value.as_array().ok_or_else(|| format!("{path} must be an array"))?;
            cardinality(schema, values.len(), "minItems", "maxItems", path)?;
            for (index, child) in values.iter().enumerate() {
                validate_value(&schema["items"], child, &format!("{path}/{index}"), depth + 1)?;
            }
        }
        Some("string") => {
            let text = value.as_str().ok_or_else(|| format!("{path} must be text"))?;
            cardinality(schema, text.chars().count(), "minLength", "maxLength", path)?;
        }
        Some("integer") => {
            let number = value.as_i64().ok_or_else(|| format!("{path} must be an integer"))?;
            if schema["minimum"].as_i64().is_some_and(|min| number < min)
                || schema["maximum"].as_i64().is_some_and(|max| number > max)
            {
                return Err(format!("{path} is outside the declared numeric range"));
            }
        }
        Some("boolean") if value.is_boolean() => {}
        Some("boolean") => return Err(format!("{path} must be a Boolean")),
        _ => return Err("unsupported host schema".to_owned()),
    }
    if schema.get("enum").and_then(Value::as_array).is_some_and(|choices| !choices.contains(value))
    {
        return Err(format!("{path} is not a declared enum value"));
    }
    Ok(())
}

fn validate_object(
    schema: &Value,
    properties: &Map<String, Value>,
    values: &Map<String, Value>,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    for required in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
        let name = required.as_str().ok_or("invalid host schema")?;
        if !values.contains_key(name) {
            return Err(format!("{path}/{name} is required"));
        }
    }
    for (name, value) in values {
        let child =
            properties.get(name).ok_or_else(|| format!("{path} contains an undeclared field"))?;
        validate_value(child, value, &format!("{path}/{name}"), depth + 1)?;
    }
    Ok(())
}

fn cardinality(
    schema: &Value,
    count: usize,
    minimum: &str,
    maximum: &str,
    path: &str,
) -> Result<(), String> {
    let count = u64::try_from(count).map_err(|_| "argument length overflow")?;
    if schema[minimum].as_u64().is_some_and(|min| count < min)
        || schema[maximum].as_u64().is_some_and(|max| count > max)
    {
        Err(format!("{path} is outside the declared length range"))
    } else {
        Ok(())
    }
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
                json!({"program":"cargo", "args":[], "purpose":"verification", "timeout_seconds":601}),
            ),
            ("workspace_scope", json!({"paths":[]})),
        ] {
            assert!(validate(name, &arguments).is_err(), "{name}: {arguments}");
        }
        assert!(validate("workspace_list", &json!({})).is_ok());
        assert!(
            validate("workspace_write", &json!({"path":"a", "content":"é".repeat(100_000)}))
                .is_ok(),
            "preserve existing file size limits"
        );
    }
}
