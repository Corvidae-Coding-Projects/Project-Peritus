//! Exact version-one Git schemas.

use peritus_tool_protocol::{BoundedJson, JsonLimits, Schema, SchemaProperty};

use crate::{GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass};

pub fn status_schema() -> Result<Schema, GitToolError> {
    object(Vec::new())
}

pub fn diff_schema() -> Result<Schema, GitToolError> {
    object(vec![
        property("base_revision", Schema::string(1, 1_024).map_err(|_| schema_error())?, true)?,
        property("maximum_entries", integer(1, 100_000)?, true)?,
        property("maximum_patch_bytes", integer(1, 8 * 1_024 * 1_024)?, true)?,
    ])
}

pub fn history_schema() -> Result<Schema, GitToolError> {
    object(vec![property("maximum_commits", integer(1, 1_024)?, true)?])
}

pub fn candidate_schema() -> Result<Schema, GitToolError> {
    object(vec![property("snapshot_id", identifier()?, true)?])
}

pub fn snapshot_schema() -> Result<Schema, GitToolError> {
    object(vec![
        property("kind", enumeration(&["current", "retained"])?, true)?,
        property("snapshot_id", identifier()?, false)?,
    ])
}

pub fn rollback_schema() -> Result<Schema, GitToolError> {
    object(vec![
        property("successor_snapshot_id", identifier()?, true)?,
        property("target_snapshot_id", identifier()?, true)?,
    ])
}

pub fn merge_schema() -> Result<Schema, GitToolError> {
    object(vec![
        property("expected_target_commit", object_id()?, true)?,
        property("source_snapshot_id", identifier()?, true)?,
        property("target_ref", Schema::string(1, 1_024).map_err(|_| schema_error())?, true)?,
    ])
}

fn identifier() -> Result<Schema, GitToolError> {
    Schema::string(32, 32).map_err(|_| schema_error())
}

fn object_id() -> Result<Schema, GitToolError> {
    Schema::string(40, 64).map_err(|_| schema_error())
}

fn enumeration(values: &[&str]) -> Result<Schema, GitToolError> {
    let values = values
        .iter()
        .map(|value| {
            BoundedJson::string((*value).to_owned(), JsonLimits::PRODUCTION)
                .map_err(|_| schema_error())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Schema::string(1, 32).and_then(|schema| schema.with_enum(values)).map_err(|_| schema_error())
}

fn integer(minimum: i64, maximum: i64) -> Result<Schema, GitToolError> {
    Schema::integer(Some(minimum), Some(maximum)).map_err(|_| schema_error())
}

fn property(name: &str, schema: Schema, required: bool) -> Result<SchemaProperty, GitToolError> {
    SchemaProperty::new(name.to_owned(), schema, required).map_err(|_| schema_error())
}

fn object(properties: Vec<SchemaProperty>) -> Result<Schema, GitToolError> {
    Schema::object(properties, false).map_err(|_| schema_error())
}

const fn schema_error() -> GitToolError {
    GitToolError::new(
        GitToolErrorKind::Protocol,
        GitToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "frozen Git schema is invalid",
    )
}
