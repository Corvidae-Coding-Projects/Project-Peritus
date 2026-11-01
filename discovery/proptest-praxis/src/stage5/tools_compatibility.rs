//! Independent structural compatibility after root enum replacement, without owner predicates.
use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Normalized {
    kind: Kind,
    values: Vec<Value>,
}
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Null,
    Bool,
    Int(Option<i64>, Option<i64>),
    Text(u32, u32),
    Array(Box<Normalized>, u32, u32),
    Object(Vec<(String, bool, Normalized)>, bool),
}
fn enum_json_fits(value: &Value) -> bool {
    let limits = JsonLimits::PRODUCTION;
    if !serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= limits.max_bytes()) {
        return false;
    }
    let mut pending = vec![(value, 1)];
    let mut members = 0_usize;
    while let Some((value, depth)) = pending.pop() {
        if depth > limits.max_depth() {
            return false;
        }
        match value {
            Value::String(text) if text.len() > limits.max_string_bytes() => return false,
            Value::Array(values) => {
                members += values.len();
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                members += values.len();
                if values.keys().any(|key| key.len() > limits.max_string_bytes()) {
                    return false;
                }
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
        if members > limits.max_members() {
            return false;
        }
    }
    true
}
fn matches_kind(kind: &Kind, value: &Value, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match (kind, value) {
        (Kind::Null, Value::Null) | (Kind::Bool, Value::Bool(_)) => true,
        (Kind::Int(min, max), Value::Number(number)) => number
            .as_i64()
            .is_some_and(|n| min.is_none_or(|min| n >= min) && max.is_none_or(|max| n <= max)),
        (Kind::Text(min, max), Value::String(text)) => {
            u64::try_from(text.len()).is_ok_and(|n| n >= u64::from(*min) && n <= u64::from(*max))
        }
        (Kind::Array(item, min, max), Value::Array(values)) => {
            u64::try_from(values.len()).is_ok_and(|n| n >= u64::from(*min) && n <= u64::from(*max))
                && values.iter().all(|value| matches_schema(item, value, depth + 1))
        }
        (Kind::Object(properties, extra), Value::Object(values)) => {
            properties.iter().all(|(key, required, _)| !*required || values.contains_key(key))
                && values.iter().all(|(key, value)| {
                    match properties.iter().find(|(name, _, _)| name == key) {
                        Some((_, _, shape)) => matches_schema(shape, value, depth + 1),
                        None => *extra,
                    }
                })
        }
        _ => false,
    }
}
fn matches_schema(schema: &Normalized, value: &Value, depth: usize) -> bool {
    matches_kind(&schema.kind, value, depth)
        && (schema.values.is_empty() || schema.values.contains(value))
}
fn normalized(shape: &Shape) -> Result<Normalized, ()> {
    let kind = match shape {
        Shape::Null => Kind::Null,
        Shape::Bool => Kind::Bool,
        Shape::Int(a, b) => {
            if matches!((a,b),(Some(a),Some(b)) if a>b) {
                return Err(());
            }
            Kind::Int(*a, *b)
        }
        Shape::Text(a, b) => {
            if a > b {
                return Err(());
            }
            Kind::Text(*a, *b)
        }
        Shape::Array(item, a, b) => {
            if a > b {
                return Err(());
            }
            Kind::Array(Box::new(normalized(item)?), *a, *b)
        }
        Shape::Object(properties, extra) => {
            if properties.len() > 256
                || properties.windows(2).any(|pair| pair[0].0.as_bytes() >= pair[1].0.as_bytes())
                || properties.iter().any(|(key, _, _)| {
                    key.is_empty() || key.len() > 256 || key.chars().any(char::is_control)
                })
            {
                return Err(());
            }
            let properties = properties
                .iter()
                .map(|(key, required, shape)| Ok((key.clone(), *required, normalized(shape)?)))
                .collect::<Result<Vec<_>, ()>>()?;
            Kind::Object(properties, *extra)
        }
        Shape::Enum(base, nodes) => {
            // Every inner constructor must first be valid. A subsequent root with_enum replaces
            // that root's enum restriction; it does not erase restrictions on child properties.
            let mut base = normalized(base)?;
            if nodes.is_empty() || nodes.len() > 256 {
                return Err(());
            }
            let values = nodes
                .iter()
                .map(|node| value(node).map_err(|_| ()))
                .collect::<Result<Vec<_>, ()>>()?;
            if values
                .iter()
                .any(|value| !enum_json_fits(value) || !matches_kind(&base.kind, value, 1))
            {
                return Err(());
            }
            let bytes = values
                .iter()
                .map(serde_json::to_vec)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ())?;
            if bytes.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(());
            }
            base.values = values;
            return Ok(base);
        }
    };
    Ok(Normalized { kind, values: vec![] })
}
fn classify(before: &Normalized, after: &Normalized) -> SchemaCompatibility {
    if before == after {
        return SchemaCompatibility::Equal;
    }
    if before.values != after.values {
        return SchemaCompatibility::Breaking;
    }
    match (&before.kind, &after.kind) {
        (Kind::Object(old, old_extra), Kind::Object(new, new_extra))
            if old_extra == new_extra
                && old.len() < new.len()
                && old.iter().all(|row| new.contains(row))
                && new
                    .iter()
                    .filter(|(key, _, _)| !old.iter().any(|(old, _, _)| old == key))
                    .all(|(_, required, _)| !*required) =>
        {
            SchemaCompatibility::Additive
        }
        _ => SchemaCompatibility::Breaking,
    }
}
pub(super) fn check(a: &Shape, b: &Shape) -> Result<crate::Metrics, String> {
    let expected_a = normalized(a);
    let expected_b = normalized(b);
    let actual_a = native_shape(a);
    let actual_b = native_shape(b);
    if expected_a.is_ok() != actual_a.is_ok() || expected_b.is_ok() != actual_b.is_ok() {
        return Err(format!("compatibility fixture constructor truth differs: a={a:?} b={b:?}"));
    }
    let (Ok(expected_a), Ok(expected_b)) = (expected_a, expected_b) else {
        return Ok(crate::Metrics::from([("invalid_compatibility_inputs".into(), 1)]));
    };
    let expected = classify(&expected_a, &expected_b);
    let actual = actual_a.map_err(err)?.compatibility_with(&actual_b.map_err(err)?);
    if actual != expected {
        return Err(format!(
            "independent normalized compatibility expected{expected:?}, actual{actual:?}: a={a:?} b={b:?}"
        ));
    }
    Ok(crate::Metrics::from([(format!("compatibility_{expected:?}"), 1)]))
}
pub(super) fn examples() -> Vec<Case> {
    let before = Shape::Object(vec![("a".into(), true, Shape::Bool)], false);
    let after = Shape::Object(
        vec![("a".into(), true, Shape::Bool), ("b".into(), false, Shape::Bool)],
        false,
    );
    let required = Shape::Object(
        vec![("a".into(), true, Shape::Bool), ("b".into(), true, Shape::Bool)],
        false,
    );
    let first = vec![Node::Object(vec![("a".into(), Node::Bool(true))])];
    let second = vec![Node::Object(vec![("a".into(), Node::Bool(false))])];
    let restricted_before = Shape::Enum(Box::new(before.clone()), first.clone());
    let restricted_after = Shape::Enum(Box::new(after.clone()), first.clone());
    let changed = Shape::Enum(Box::new(after), second.clone());
    let required = Shape::Enum(
        Box::new(required),
        vec![Node::Object(vec![("a".into(), Node::Bool(true)), ("b".into(), Node::Bool(true))])],
    );
    let replaced = Shape::Enum(Box::new(restricted_before.clone()), second.clone());
    let direct = Shape::Enum(Box::new(before), second);
    vec![
        Case::Compatibility { before: restricted_before.clone(), after: restricted_after.clone() },
        Case::Compatibility { before: restricted_after.clone(), after: restricted_before.clone() },
        Case::Compatibility { before: restricted_before.clone(), after: restricted_before.clone() },
        Case::Compatibility { before: restricted_before.clone(), after: changed },
        Case::Compatibility { before: restricted_before.clone(), after: required },
        Case::Compatibility { before: replaced, after: direct },
        Case::Compatibility {
            before: Shape::Enum(Box::new(restricted_before.clone()), vec![]),
            after: restricted_before,
        },
    ]
}
