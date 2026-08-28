//! Provider-neutral workspace tool schemas.

use peritus_model_protocol::{
    BoundedText, JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub fn definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path.",
            r#"{"additionalProperties":false,"properties":{"depth":{"type":"integer"},"path":{"type":"string"}},"type":"object"}"#,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching lines.",
            r#"{"additionalProperties":false,"properties":{"max_results":{"type":"integer"},"path":{"type":"string"},"query":{"type":"string"}},"required":["query"],"type":"object"}"#,
        ),
        (
            "workspace_read",
            "Read a bounded line range from one workspace-relative text file.",
            r#"{"additionalProperties":false,"properties":{"end_line":{"type":"integer"},"path":{"type":"string"},"start_line":{"type":"integer"}},"required":["path"],"type":"object"}"#,
        ),
        (
            "workspace_write",
            "Create or completely replace one workspace-relative text file.",
            r#"{"additionalProperties":false,"properties":{"content":{"type":"string"},"path":{"type":"string"}},"required":["content","path"],"type":"object"}"#,
        ),
        (
            "workspace_patch",
            "Replace an exact text fragment in one workspace-relative file. By default the old fragment must occur exactly once.",
            r#"{"additionalProperties":false,"properties":{"new":{"type":"string"},"old":{"type":"string"},"path":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["new","old","path"],"type":"object"}"#,
        ),
        (
            "workspace_remove",
            "Remove one exact workspace-relative regular file after reading it. Files that appear during the invocation through commands, services, hooks, or evaluators are external evidence and cannot be removed.",
            r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
        ),
        (
            "run_command",
            "Run a non-destructive structured executable and argv in the managed workspace; use it to build, test, lint, inspect Git, and observe failures. Use workspace_remove for intentional file deletion.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"cwd":{"type":"string"},"program":{"type":"string"}},"required":["args","program"],"type":"object"}"#,
        ),
    ])
}

/// Returns the repository-inspection subset used by the mandatory design pass.
pub fn read_only_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path.",
            r#"{"additionalProperties":false,"properties":{"depth":{"type":"integer"},"path":{"type":"string"}},"type":"object"}"#,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching lines.",
            r#"{"additionalProperties":false,"properties":{"max_results":{"type":"integer"},"path":{"type":"string"},"query":{"type":"string"}},"required":["query"],"type":"object"}"#,
        ),
        (
            "workspace_read",
            "Read a bounded line range from one workspace-relative text file.",
            r#"{"additionalProperties":false,"properties":{"end_line":{"type":"integer"},"path":{"type":"string"},"start_line":{"type":"integer"}},"required":["path"],"type":"object"}"#,
        ),
    ])
}

fn definitions_from(
    definitions: &[(&str, &str, &str)],
) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions
        .iter()
        .copied()
        .map(|(name, description, schema)| definition(name, description, schema))
        .collect()
}

fn definition(
    name: &str,
    description: &str,
    schema: &str,
) -> Result<ToolDefinition, ProductRunnerError> {
    let limits = ProtocolLimits::PRODUCTION;
    Ok(ToolDefinition::new(
        ToolName::new(name.to_owned()).map_err(|error| protocol(&error))?,
        Some(BoundedText::new(description.to_owned(), limits).map_err(|error| protocol(&error))?),
        JsonSchema::parse(schema, SchemaDialect::Draft202012, JsonBounds::schema(limits))
            .map_err(|error| protocol(&error))?,
        true,
    ))
}

fn protocol(error: &peritus_model_protocol::ProtocolError) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "construct developer tool catalog",
        error.to_string(),
    )
}
