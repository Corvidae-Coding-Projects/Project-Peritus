//! Host admission for the bounded JSON Schema vocabulary used by model tools.
//!
//! A provider's constrained decoding is an optimization, never execution authority.

use std::{collections::BTreeSet, fmt};

use serde_json::Value;

use crate::{CanonicalJson, JsonSchema};
mod projection;

const MAX_DEPTH: usize = 64;
const ANNOTATIONS: &[&str] =
    &["$schema", "$id", "$comment", "title", "description", "default", "examples", "deprecated"];
const KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minProperties",
    "maxProperties",
    "uniqueItems",
    "anyOf",
    "oneOf",
    "allOf",
    "not",
    "$ref",
    "$defs",
    "definitions",
];

/// A payload-free, field-addressed admission failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolArgumentViolation {
    path: String,
    detail: String,
}

impl ToolArgumentViolation {
    /// Constructs a contract failure without copying argument values.
    #[must_use]
    pub fn new(path: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { path: path.into(), detail: detail.into() }
    }
    /// JSON Pointer relative to the submitted arguments (empty means the root).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The violated contract, without copying argument values into diagnostics.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ToolArgumentViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", if self.path.is_empty() { "/" } else { &self.path }, self.detail)
    }
}
impl std::error::Error for ToolArgumentViolation {}

/// A checked tool schema. Unknown validation keywords are rejected, never ignored.
#[derive(Clone, Debug)]
pub struct ToolArgumentSchema {
    root: Value,
}

impl ToolArgumentSchema {
    /// Compiles the host vocabulary, including local references and nullable/tagged unions.
    ///
    /// # Errors
    /// Rejects unsupported or malformed schemas before any model request or tool effect.
    pub fn compile(schema: &JsonSchema) -> Result<Self, ToolArgumentViolation> {
        let root: Value = serde_json::from_slice(schema.canonical_bytes())
            .map_err(|_| violation("", "invalid tool schema JSON"))?;
        check(&root, &root, "", 0, &mut BTreeSet::new())?;
        Ok(Self { root })
    }

    /// Validates exactly the submitted call, independently of provider enforcement.
    ///
    /// # Errors
    /// Identifies a violated constraint at a pointer into the submitted arguments.
    pub fn validate(&self, arguments: &CanonicalJson) -> Result<(), ToolArgumentViolation> {
        let value: Value = serde_json::from_slice(arguments.canonical_bytes())
            .map_err(|_| violation("", "invalid argument JSON"))?;
        self.validate_value(&value)
    }

    /// Validates an already decoded proposal without reserializing it.
    ///
    /// # Errors
    /// Returns the first violated field constraint; no effects are performed.
    pub fn validate_value(&self, value: &Value) -> Result<(), ToolArgumentViolation> {
        let mut budget = 200_000_usize;
        let result = validate(&self.root, &self.root, value, "", 0, &mut budget);
        if budget == 0 {
            return Err(violation("", "argument validation work bound exceeded"));
        }
        result
    }
}

fn violation(path: &str, detail: impl Into<String>) -> ToolArgumentViolation {
    ToolArgumentViolation { path: path.to_owned(), detail: detail.into() }
}

fn pointer(path: &str, name: &str) -> String {
    format!("{path}/{}", name.replace('~', "~0").replace('/', "~1"))
}

fn reference<'a>(root: &'a Value, text: &str) -> Option<&'a Value> {
    text.strip_prefix('#').and_then(|p| root.pointer(p))
}

fn check(
    root: &Value,
    schema: &Value,
    path: &str,
    depth: usize,
    seen: &mut BTreeSet<String>,
) -> Result<(), ToolArgumentViolation> {
    if depth > MAX_DEPTH {
        return Err(violation(path, "tool schema depth exceeded"));
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let object =
        schema.as_object().ok_or_else(|| violation(path, "schema must be an object or boolean"))?;
    for key in object.keys() {
        if !KEYWORDS.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
            return Err(violation(&pointer(path, key), "unsupported tool schema keyword"));
        }
    }
    if let Some(value) = object.get("$ref") {
        let name = value.as_str().ok_or_else(|| violation(path, "invalid schema reference"))?;
        let target = reference(root, name)
            .ok_or_else(|| violation(path, "unresolved local schema reference"))?;
        if seen.insert(name.to_owned()) {
            check(root, target, path, depth + 1, seen)?;
        }
    }
    if let Some(types) = object.get("type") {
        let valid = |v: &Value| {
            v.as_str().is_some_and(|s| {
                ["object", "array", "string", "integer", "number", "boolean", "null"].contains(&s)
            })
        };
        if !(valid(types) || types.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(valid)))
        {
            return Err(violation(path, "unsupported schema type"));
        }
    }
    for field in ["properties", "$defs", "definitions"] {
        if let Some(values) = object.get(field) {
            for (name, child) in
                values.as_object().ok_or_else(|| violation(path, "invalid schema property map"))?
            {
                check(root, child, &pointer(path, name), depth + 1, seen)?;
            }
        }
    }
    if let Some(required) = object.get("required") {
        let items =
            required.as_array().ok_or_else(|| violation(path, "invalid required field list"))?;
        let mut names = BTreeSet::new();
        for item in items {
            let name =
                item.as_str().ok_or_else(|| violation(path, "invalid required field name"))?;
            if !names.insert(name) {
                return Err(violation(path, "duplicate required field"));
            }
        }
    }
    for field in ["items", "additionalProperties", "not"] {
        if let Some(child) = object.get(field) {
            check(root, child, path, depth + 1, seen)?;
        }
    }
    for field in ["anyOf", "oneOf", "allOf"] {
        if let Some(values) = object.get(field) {
            let children = values
                .as_array()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| violation(path, "invalid schema alternatives"))?;
            for child in children {
                check(root, child, path, depth + 1, seen)?;
            }
        }
    }
    for field in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
        if object.get(field).is_some_and(|v| !v.is_number()) {
            return Err(violation(path, "invalid numeric schema bound"));
        }
    }
    for field in
        ["minItems", "maxItems", "minLength", "maxLength", "minProperties", "maxProperties"]
    {
        if object.get(field).is_some_and(|v| v.as_u64().is_none()) {
            return Err(violation(path, "invalid cardinality schema bound"));
        }
    }
    if object.get("uniqueItems").is_some_and(|v| !v.is_boolean()) {
        return Err(violation(path, "invalid uniqueness constraint"));
    }
    if object.get("enum").is_some_and(|v| v.as_array().is_none_or(Vec::is_empty)) {
        return Err(violation(path, "invalid schema enum"));
    }
    Ok(())
}

fn matches_type(name: &str, value: &Value) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|v| v.fract() == 0.0)
        }
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn branch_score(schema: &Value, value: &Value) -> usize {
    schema["properties"].as_object().map_or(0, |props| {
        props
            .iter()
            .map(|(k, s)| {
                usize::from(value.get(k).is_some_and(|v| {
                    s.get("const").is_some_and(|c| equivalent(c, v))
                        || s["enum"].as_array().is_some_and(|a| a.iter().any(|c| equivalent(c, v)))
                }))
            })
            .sum()
    })
}

fn bound(
    schema: &Value,
    path: &str,
    len: usize,
    min: &str,
    max: &str,
) -> Result<(), ToolArgumentViolation> {
    let len = u64::try_from(len).map_err(|_| violation(path, "value size overflow"))?;
    if schema[min].as_u64().is_some_and(|v| len < v)
        || schema[max].as_u64().is_some_and(|v| len > v)
    {
        return Err(violation(path, format!("size violates {min}/{max}")));
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "one bounded recursive dispatch keeps all supported constraints visible"
)]
fn validate(
    root: &Value,
    schema: &Value,
    value: &Value,
    path: &str,
    depth: usize,
    budget: &mut usize,
) -> Result<(), ToolArgumentViolation> {
    *budget = budget.saturating_sub(1);
    if *budget == 0 {
        return Err(violation(path, "argument validation work bound exceeded"));
    }
    if depth > MAX_DEPTH {
        return Err(violation(path, "argument validation depth exceeded"));
    }
    if schema == &Value::Bool(true) {
        return Ok(());
    }
    if schema == &Value::Bool(false) {
        return Err(violation(path, "value is forbidden"));
    }
    if let Some(name) = schema["$ref"].as_str() {
        validate(
            root,
            reference(root, name).ok_or_else(|| violation(path, "unresolved schema reference"))?,
            value,
            path,
            depth + 1,
            budget,
        )?;
    }
    for field in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema[field].as_array() {
            let mut successes = 0;
            let mut failures = Vec::new();
            for branch in branches {
                match validate(root, branch, value, path, depth + 1, budget) {
                    Ok(()) => successes += 1,
                    Err(error) => failures.push((branch_score(branch, value), error)),
                }
            }
            let accepted = match field {
                "anyOf" => successes > 0,
                "oneOf" => successes == 1,
                _ => successes == branches.len(),
            };
            if !accepted {
                if let Some((_, error)) = failures
                    .into_iter()
                    .max_by_key(|(score, e)| (*score, e.path.matches('/').count()))
                {
                    return Err(error);
                }
                return Err(violation(path, format!("value violates {field}")));
            }
        }
    }
    if schema
        .get("not")
        .is_some_and(|child| validate(root, child, value, path, depth + 1, budget).is_ok())
    {
        return Err(violation(path, "value matches a forbidden schema"));
    }
    if let Some(types) = schema.get("type") {
        let matches = types.as_str().is_some_and(|t| matches_type(t, value))
            || types.as_array().is_some_and(|a| {
                a.iter().any(|t| t.as_str().is_some_and(|t| matches_type(t, value)))
            });
        if !matches {
            return Err(violation(path, "value has the wrong type"));
        }
    }
    if schema.get("const").is_some_and(|v| !equivalent(v, value))
        || schema["enum"].as_array().is_some_and(|a| !a.iter().any(|v| equivalent(v, value)))
    {
        return Err(violation(path, "value is not a declared alternative"));
    }
    if let Some(values) = value.as_object() {
        bound(schema, path, values.len(), "minProperties", "maxProperties")?;
        if let Some(required) = schema["required"].as_array() {
            for name in required.iter().filter_map(Value::as_str) {
                if !values.contains_key(name) {
                    return Err(violation(&pointer(path, name), "required field is missing"));
                }
            }
        }
        for (name, child) in values {
            if let Some(s) = schema["properties"].get(name) {
                validate(root, s, child, &pointer(path, name), depth + 1, budget)?;
            } else if let Some(s) = schema.get("additionalProperties") {
                validate(root, s, child, &pointer(path, name), depth + 1, budget)?;
            }
        }
    }
    if let Some(values) = value.as_array() {
        bound(schema, path, values.len(), "minItems", "maxItems")?;
        for (index, child) in values.iter().enumerate() {
            if schema["uniqueItems"] == true {
                for previous in &values[..index] {
                    *budget = budget.saturating_sub(1);
                    if *budget == 0 {
                        return Err(violation(path, "argument validation work bound exceeded"));
                    }
                    if equivalent(previous, child) {
                        return Err(violation(path, "array entries must be unique"));
                    }
                }
            }
            if let Some(s) = schema.get("items") {
                validate(root, s, child, &pointer(path, &index.to_string()), depth + 1, budget)?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        bound(schema, path, text.chars().count(), "minLength", "maxLength")?;
    }
    if let Some(number) = value.as_number() {
        for (name, accepted) in [
            ("minimum", true),
            ("maximum", false),
            ("exclusiveMinimum", true),
            ("exclusiveMaximum", false),
        ] {
            if let Some(limit) = schema[name].as_number() {
                let exclusive = name.starts_with("exclusive");
                let order = compare_number(number, limit);
                if (accepted && (order.is_lt() || exclusive && order.is_eq()))
                    || (!accepted && (order.is_gt() || exclusive && order.is_eq()))
                {
                    return Err(violation(path, "number is outside its declared range"));
                }
            }
        }
    }
    Ok(())
}

fn equivalent(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => compare_number(a, b).is_eq(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|b| equivalent(v, b)))
        }
        _ => left == right,
    }
}

// Compare the represented decimal numbers, preserving integer boundaries above 2^53.
fn compare_number(left: &serde_json::Number, right: &serde_json::Number) -> std::cmp::Ordering {
    fn decimal(number: &serde_json::Number) -> (bool, i32, Vec<u8>) {
        let text = number.to_string();
        let negative = text.starts_with('-');
        let unsigned = text.trim_start_matches('-');
        let (significand, exponent) = unsigned
            .split_once(['e', 'E'])
            .map_or((unsigned, 0), |(n, e)| (n, e.parse::<i32>().unwrap_or(0)));
        let point = significand.find('.').unwrap_or(significand.len());
        let mut digits = significand.bytes().filter(|b| *b != b'.').collect::<Vec<_>>();
        let leading = digits.iter().take_while(|b| **b == b'0').count();
        if leading == digits.len() {
            return (false, 0, vec![b'0']);
        }
        digits.drain(..leading);
        // serde_json's finite numeric spelling is bounded to a few dozen characters.
        let magnitude = i32::try_from(point).unwrap_or(i32::MAX) + exponent
            - i32::try_from(leading).unwrap_or(0);
        (negative, magnitude, digits)
    }
    let (ln, le, ld) = decimal(left);
    let (rn, re, rd) = decimal(right);
    if ld == b"0" {
        return if rd == b"0" {
            std::cmp::Ordering::Equal
        } else if rn {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        };
    }
    if rd == b"0" {
        return if ln { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
    }
    let order = ln.cmp(&rn).reverse();
    if !order.is_eq() {
        return order;
    }
    let order = le.cmp(&re).then_with(|| {
        (0..ld.len().max(rd.len()))
            .map(|i| ld.get(i).copied().unwrap_or(b'0').cmp(&rd.get(i).copied().unwrap_or(b'0')))
            .find(|o| !o.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if ln { order.reverse() } else { order }
}
