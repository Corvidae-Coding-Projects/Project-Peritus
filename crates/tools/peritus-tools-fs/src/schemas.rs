//! Exact version-one filesystem schemas.

use peritus_tool_protocol::{BoundedJson, JsonLimits, Schema, SchemaProperty};

use crate::{FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass};

const PATH_MAX: u32 = 4_096;
const CONTENT_MAX: u32 = 65_536;

pub fn discover_schema() -> Result<Schema, FsToolError> {
    object(vec![
        property("maximum_depth", integer(1, 64)?, true)?,
        property("maximum_entries", integer(1, 100_000)?, true)?,
        property("root", path()?, false)?,
    ])
}

pub fn metadata_schema() -> Result<Schema, FsToolError> {
    object(vec![property("path", path()?, true)?])
}

pub fn read_schema() -> Result<Schema, FsToolError> {
    object(vec![
        property("maximum_bytes", integer(1, 48 * 1_024)?, true)?,
        property("path", path()?, true)?,
    ])
}

pub fn search_schema() -> Result<Schema, FsToolError> {
    object(vec![
        property("case_sensitive", Schema::boolean(), true)?,
        property("literal", Schema::string(1, 4_096).map_err(|_| schema_error())?, true)?,
        property("maximum_depth", integer(1, 64)?, true)?,
        property("maximum_entries", integer(1, 100_000)?, true)?,
        property("maximum_file_bytes", integer(1, 8 * 1_024 * 1_024)?, true)?,
        property("maximum_matches", integer(1, 10_000)?, true)?,
        property("maximum_total_bytes", integer(1, 64 * 1_024 * 1_024)?, true)?,
        property("root", path()?, false)?,
    ])
}

pub fn create_schema() -> Result<Schema, FsToolError> {
    final_file_schema(false)
}

pub fn write_schema() -> Result<Schema, FsToolError> {
    final_file_schema(true)
}

pub fn remove_schema() -> Result<Schema, FsToolError> {
    object(vec![
        property("path", path()?, true)?,
        property("preimage", preimage_schema(false)?, true)?,
    ])
}

pub fn replace_schema() -> Result<Schema, FsToolError> {
    replacement_schema()
}

pub fn patch_schema() -> Result<Schema, FsToolError> {
    let edit = object(vec![
        property("content", Schema::string(0, CONTENT_MAX).map_err(|_| schema_error())?, false)?,
        property("content_encoding", content_encoding()?, false)?,
        property("line_endings", line_endings()?, false)?,
        property("mode", mode()?, false)?,
        property("operation", edit_operation()?, true)?,
        property("path", path()?, true)?,
        property("preimage", preimage_schema(true)?, false)?,
    ])?;
    object(vec![property(
        "edits",
        Schema::array(edit, 1, 1_024).map_err(|_| schema_error())?,
        true,
    )?])
}

fn final_file_schema(include_preimage: bool) -> Result<Schema, FsToolError> {
    let mut properties = vec![
        property("content", Schema::string(0, CONTENT_MAX).map_err(|_| schema_error())?, true)?,
        property("content_encoding", content_encoding()?, true)?,
        property("line_endings", line_endings()?, true)?,
        property("mode", mode()?, true)?,
        property("path", path()?, true)?,
    ];
    if include_preimage {
        properties.push(property("preimage", preimage_schema(true)?, true)?);
        properties.sort_unstable_by(|left, right| left.name().cmp(right.name()));
    }
    object(properties)
}

fn replacement_schema() -> Result<Schema, FsToolError> {
    object(vec![
        property("content", Schema::string(0, CONTENT_MAX).map_err(|_| schema_error())?, true)?,
        property("content_encoding", content_encoding()?, true)?,
        property("line_endings", line_endings()?, true)?,
        property("mode", mode()?, true)?,
        property("path", path()?, true)?,
        property("preimage", preimage_schema(false)?, true)?,
    ])
}

fn preimage_schema(allow_absent: bool) -> Result<Schema, FsToolError> {
    object(vec![
        property("digest", Schema::string(64, 64).map_err(|_| schema_error())?, false)?,
        property("mode", mode()?, false)?,
        property("size", integer(0, 8 * 1_024 * 1_024)?, false)?,
        property(
            "state",
            enumeration(if allow_absent { &["absent", "present"][..] } else { &["present"][..] })?,
            true,
        )?,
    ])
}

fn path() -> Result<Schema, FsToolError> {
    Schema::string(1, PATH_MAX).map_err(|_| schema_error())
}

fn mode() -> Result<Schema, FsToolError> {
    enumeration(&["executable", "regular"])
}

fn line_endings() -> Result<Schema, FsToolError> {
    enumeration(&["crlf", "lf", "preserve"])
}

fn content_encoding() -> Result<Schema, FsToolError> {
    enumeration(&["base64", "utf8"])
}

fn edit_operation() -> Result<Schema, FsToolError> {
    enumeration(&["create", "remove", "replace"])
}

fn enumeration(values: &[&str]) -> Result<Schema, FsToolError> {
    let values = values
        .iter()
        .map(|value| {
            BoundedJson::string((*value).to_owned(), JsonLimits::PRODUCTION)
                .map_err(|_| schema_error())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Schema::string(1, 32).and_then(|schema| schema.with_enum(values)).map_err(|_| schema_error())
}

fn integer(minimum: i64, maximum: i64) -> Result<Schema, FsToolError> {
    Schema::integer(Some(minimum), Some(maximum)).map_err(|_| schema_error())
}

fn property(name: &str, schema: Schema, required: bool) -> Result<SchemaProperty, FsToolError> {
    SchemaProperty::new(name.to_owned(), schema, required).map_err(|_| schema_error())
}

fn object(properties: Vec<SchemaProperty>) -> Result<Schema, FsToolError> {
    Schema::object(properties, false).map_err(|_| schema_error())
}

const fn schema_error() -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Protocol,
        FsToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "frozen filesystem schema is invalid",
    )
}
