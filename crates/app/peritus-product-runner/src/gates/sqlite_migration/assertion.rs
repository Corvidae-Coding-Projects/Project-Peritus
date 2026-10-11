//! Parsing and execution of explicitly selected `SQLite` scalar postchecks.

use rusqlite::{Connection, types::ValueRef};
use serde_json::Value;

use super::unquote;

#[derive(Debug)]
pub(super) struct SqliteAssertion {
    query: String,
    expected: Value,
}

pub(super) fn parse_assertion(raw: &str) -> Result<SqliteAssertion, String> {
    let (query, expected) = raw.split_once("=>").ok_or_else(|| {
        "SQLite assertion must be a single read-only query followed by `=>` and one JSON scalar"
            .to_owned()
    })?;
    let query = unquote(query.trim()).unwrap_or_else(|| query.trim()).to_owned();
    let expected = serde_json::from_str::<Value>(expected.trim())
        .map_err(|error| format!("parse expected SQLite assertion value: {error}"))?;
    if !matches!(expected, Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)) {
        return Err("SQLite assertion expected value must be a JSON scalar".to_owned());
    }
    Ok(SqliteAssertion { query, expected })
}

pub(super) fn check_assertion(
    connection: &Connection,
    assertion: &SqliteAssertion,
    number: usize,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(&assertion.query)
        .map_err(|error| format!("prepare postcheck assertion {number}: {error}"))?;
    if !statement.readonly() || statement.column_count() != 1 {
        return Err(format!("postcheck assertion {number} must be one read-only scalar query"));
    }
    let mut rows = statement
        .query([])
        .map_err(|error| format!("run postcheck assertion {number}: {error}"))?;
    let row = rows
        .next()
        .map_err(|error| format!("read postcheck assertion {number}: {error}"))?
        .ok_or_else(|| format!("postcheck assertion {number} returned no row"))?;
    let actual =
        row.get_ref(0).map_err(|error| format!("read postcheck assertion {number}: {error}"))?;
    if !scalar_matches(actual, &assertion.expected) {
        return Err(format!("postcheck assertion {number} did not match its declared JSON scalar"));
    }
    if rows.next().map_err(|error| format!("read postcheck assertion {number}: {error}"))?.is_some()
    {
        return Err(format!("postcheck assertion {number} returned more than one row"));
    }
    Ok(())
}

fn scalar_matches(actual: ValueRef<'_>, expected: &Value) -> bool {
    match (actual, expected) {
        (ValueRef::Null, Value::Null) => true,
        (ValueRef::Integer(actual), Value::Bool(expected)) => actual == i64::from(*expected),
        (ValueRef::Integer(actual), Value::Number(expected)) => expected.as_i64() == Some(actual),
        (ValueRef::Real(actual), Value::Number(expected)) => expected.as_f64() == Some(actual),
        (ValueRef::Text(actual), Value::String(expected)) => actual == expected.as_bytes(),
        _ => false,
    }
}
