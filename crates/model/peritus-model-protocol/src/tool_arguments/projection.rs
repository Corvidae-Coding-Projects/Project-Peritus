//! Lossless optional-field projection for providers requiring closed, fully required objects.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{MAX_DEPTH, ToolArgumentSchema, ToolArgumentViolation, reference, validate, violation};

impl ToolArgumentSchema {
    /// Produces a constrained-decoding schema; absent optional fields use an explicit null marker.
    ///
    /// # Errors
    /// Rejects open maps or recursive references that this transport cannot project losslessly.
    /// Callers must report such tools as host-validated rather than provider-enforced.
    pub fn provider_schema(&self) -> Result<Value, ToolArgumentViolation> {
        project(&self.root, &self.root, 0, &mut BTreeSet::new())
    }

    /// Restores transport null markers to omitted fields and checks the original host contract.
    ///
    /// # Errors
    /// Rejects invalid required fields and ambiguous or unsupported argument shapes.
    pub fn restore_provider_value(&self, value: &mut Value) -> Result<(), ToolArgumentViolation> {
        restore(&self.root, &self.root, value, 0)?;
        self.validate_value(value)
    }
}

fn accepts(root: &Value, schema: &Value, value: &Value) -> bool {
    let mut budget = 200_000;
    validate(root, schema, value, "", 0, &mut budget).is_ok() && budget > 0
}

fn project_constants(
    object: &mut serde_json::Map<String, Value>,
) -> Result<(), ToolArgumentViolation> {
    if let Some(constant) = object.remove("const") {
        let kind = match &constant {
            Value::String(_) => "string",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
            Value::Number(_) => "number",
            _ => return Err(violation("", "compound const requires host validation")),
        };
        object.entry("type").or_insert(json!(kind));
        object.insert("enum".into(), json!([constant]));
    }
    if !object.contains_key("type")
        && let Some(values) = object.get("enum").and_then(Value::as_array)
    {
        let mut kinds = BTreeSet::new();
        for value in values {
            kinds.insert(match value {
                Value::String(_) => "string",
                Value::Bool(_) => "boolean",
                Value::Null => "null",
                Value::Number(_) => "number",
                _ => return Err(violation("", "compound enum requires host validation")),
            });
        }
        object.insert(
            "type".into(),
            if kinds.len() == 1 { json!(kinds.first()) } else { json!(kinds) },
        );
    }
    Ok(())
}

fn project(
    root: &Value,
    schema: &Value,
    depth: usize,
    active: &mut BTreeSet<String>,
) -> Result<Value, ToolArgumentViolation> {
    if depth > MAX_DEPTH {
        return Err(violation("", "provider schema depth exceeded"));
    }
    if let Some(name) = schema["$ref"].as_str() {
        if schema.as_object().is_some_and(|o| {
            o.keys().any(|k| k != "$ref" && !super::ANNOTATIONS.contains(&k.as_str()))
        }) {
            return Err(violation("", "provider projection cannot discard reference siblings"));
        }
        if !active.insert(name.into()) {
            return Err(violation("", "recursive schema requires host validation"));
        }
        let value = project(
            root,
            reference(root, name).ok_or_else(|| violation("", "unresolved schema reference"))?,
            depth + 1,
            active,
        )?;
        active.remove(name);
        return Ok(value);
    }
    if let Some(types) = schema["type"].as_array() {
        let mut branches = Vec::new();
        for kind in types {
            let mut branch = schema.clone();
            branch["type"] = kind.clone();
            branches.push(project(root, &branch, depth + 1, active)?);
        }
        return Ok(json!({"anyOf":branches}));
    }
    let mut result = schema.clone();
    let object = result
        .as_object_mut()
        .ok_or_else(|| violation("", "unconstrained schema requires host validation"))?;
    for name in super::ANNOTATIONS.iter().chain(["$defs", "definitions"].iter()) {
        object.remove(*name);
    }
    project_constants(object)?;
    if object.keys().any(|name| {
        [
            "oneOf",
            "allOf",
            "not",
            "uniqueItems",
            "minProperties",
            "maxProperties",
            "exclusiveMinimum",
            "exclusiveMaximum",
        ]
        .contains(&name.as_str())
    }) {
        return Err(violation("", "schema constraints require host validation for this transport"));
    }
    if let Some(branches) = object.get_mut("anyOf").and_then(Value::as_array_mut) {
        for branch in branches {
            *branch = project(root, branch, depth + 1, active)?;
        }
    }
    if object.get("type").and_then(Value::as_str) == Some("object") {
        if object.get("additionalProperties") != Some(&Value::Bool(false)) {
            return Err(violation("", "open object requires host validation"));
        }
        let required = schema["required"].as_array().cloned().unwrap_or_default();
        let properties = object
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| violation("", "object properties missing"))?;
        for (name, child) in properties.iter_mut() {
            let optional = !required.contains(&json!(name));
            let nullable = accepts(root, &schema["properties"][name], &Value::Null);
            if optional && nullable {
                return Err(violation(
                    "",
                    "nullable optional fields require host validation to preserve omission semantics",
                ));
            }
            *child = project(root, child, depth + 1, active)?;
            if optional && !nullable {
                *child = json!({"anyOf":[child,{"type":"null"}]});
            }
        }
        let names = properties.keys().cloned().collect::<Vec<_>>();
        object.insert("required".into(), json!(names));
    }
    if let Some(items) = object.get_mut("items") {
        *items = project(root, items, depth + 1, active)?;
    }
    if !object.contains_key("type") && !object.contains_key("anyOf") {
        return Err(violation("", "unconstrained schema requires host validation"));
    }
    Ok(result)
}

fn restore(
    root: &Value,
    schema: &Value,
    value: &mut Value,
    depth: usize,
) -> Result<(), ToolArgumentViolation> {
    if depth > MAX_DEPTH {
        return Err(violation("", "provider argument depth exceeded"));
    }
    if let Some(name) = schema["$ref"].as_str() {
        return restore(
            root,
            reference(root, name).ok_or_else(|| violation("", "unresolved schema reference"))?,
            value,
            depth + 1,
        );
    }
    if let Some(branches) = schema["anyOf"].as_array() {
        for branch in branches {
            let mut candidate = value.clone();
            if restore(root, branch, &mut candidate, depth + 1).is_ok()
                && accepts(root, branch, &candidate)
            {
                *value = candidate;
                return Ok(());
            }
        }
        return Err(violation("", "provider arguments do not match any declared variant"));
    }
    if let Some(fields) = value.as_object_mut() {
        let required = schema["required"].as_array().cloned().unwrap_or_default();
        if let Some(properties) = schema["properties"].as_object() {
            for (name, child) in properties {
                let optional = !required.contains(&json!(name));
                if optional
                    && fields.get(name) == Some(&Value::Null)
                    && !accepts(root, child, &Value::Null)
                {
                    fields.remove(name);
                } else if let Some(item) = fields.get_mut(name) {
                    restore(root, child, item, depth + 1)?;
                }
            }
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), value.as_array_mut()) {
        for value in values {
            restore(root, items, value, depth + 1)?;
        }
    }
    Ok(())
}
