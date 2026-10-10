//! Provider-neutral workspace tool schemas.

use peritus_model_protocol::{
    BoundedText, JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const WORKSPACE_LIST_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"cursor":{"additionalProperties":false,"properties":{"ordinal":{"minimum":0,"type":"integer"},"scope":{"type":"string"}},"required":["ordinal","scope"],"type":"object"},"depth":{"type":"integer","minimum":1},"max_bytes":{"type":"integer","default":16384,"minimum":256,"maximum":524288},"path":{"type":"string"}},"type":"object"}"#;
const WORKSPACE_SEARCH_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"cursor":{"additionalProperties":false,"properties":{"ordinal":{"minimum":0,"type":"integer"},"scope":{"type":"string"}},"required":["ordinal","scope"],"type":"object"},"max_bytes":{"type":"integer","default":16384,"minimum":256,"maximum":524288},"max_results":{"type":"integer","minimum":1},"path":{"type":"string"},"query":{"type":"string","minLength":1}},"required":["query"],"type":"object"}"#;
const WORKSPACE_READ_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"end_line":{"type":"integer","minimum":1},"line_byte_offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","default":16384,"minimum":256,"maximum":524288},"path":{"type":"string"},"start_line":{"type":"integer","minimum":1}},"required":["path"],"type":"object"}"#;
const ATTACHMENT_READ_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"attachment":{"type":"string","minLength":32,"maxLength":32},"version":{"type":"string","minLength":32,"maxLength":32},"source_sha256":{"type":"string","minLength":64,"maxLength":64},"selected_sha256":{"type":"string","minLength":64,"maxLength":64},"source_bytes":{"type":"integer","minimum":0},"range_start":{"type":"integer","minimum":0},"range_end":{"type":"integer","minimum":0},"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","minimum":4,"maximum":32768}},"required":["attachment","version","source_sha256","selected_sha256","source_bytes","range_start","range_end","offset","max_bytes"],"type":"object"}"#;
const REVIEW_EVIDENCE_READ_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"offset":{"type":"integer","minimum":0},"max_bytes":{"type":"integer","default":16384,"minimum":1,"maximum":524288},"section":{"type":"string","default":"developer_commands","enum":["transcript","diff","gates","developer_commands","prior","correction"]}},"required":["offset","max_bytes"],"type":"object"}"#;
const COMMAND_HANDLE_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"}},"required":["handle"],"type":"object"}"#;

pub fn definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_for_attachments(true)
}

/// Builds writer tools and advertises immutable attachment reads only when the current input
/// has an authenticated selected file version.
pub fn definitions_for_attachments(
    has_selected_file_attachments: bool,
) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    let mut definitions = definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path with current byte size and permission metadata. The result reports the exact workspace_root, path semantics, and observed execution_resources including the recommended build parallelism. Pages stay within max_bytes; pass the full next_cursor object back as cursor to continue, and inspect omissions because inaccessible entries are reported there. A cursor is bound to its path and depth. When the task names an absolute path below that root, remove the exact root prefix once instead of repeating the root directory. An exact absolute directory outside the workspace is accepted only when the user's task explicitly named it; that result is read-only reference evidence and does not ground workspace mutation or commands. Call this first with a workspace-relative path in every fresh writer or fixer turn; mutation and process tools remain locked until a successful workspace listing and a targeted workspace file read.",
            WORKSPACE_LIST_SCHEMA,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching line excerpts. Pages stay within max_bytes; pass the full next_cursor object back as cursor to continue the same path and query. Each result reports its line number, byte offset, and line length so workspace_read can retrieve an oversized line in exact byte ranges. Omitted or unreadable files are reported explicitly.",
            WORKSPACE_SEARCH_SCHEMA,
        ),
        (
            "workspace_read",
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file. Line numbers are one-based and both start_line and end_line are inclusive; the default is lines 1 through 500. Pages stay within max_bytes and report the actual last displayed line. When next_line equals end_line and next_line_byte_offset is positive, continue that oversized line with the same start_line and line_byte_offset; otherwise continue at next_line with byte offset zero. An exact absolute file outside the workspace is accepted only when it is user-named reference evidence; it remains read-only and does not ground workspace mutation or commands. Call this after a workspace listing and read the exact current workspace target before changing an existing file.",
            WORKSPACE_READ_SCHEMA,
        ),
        (
            "attachment_read",
            "Read a bounded UTF-8 page from one user-confirmed immutable attachment version. Use the exact attachment, version, source_sha256, selected_sha256, source_bytes and selected range shown in the governing input. offset is an absolute source byte offset; continue at next_offset until null. Pages contain at most 32 KiB and never reopen a workspace path or substitute newer bytes.",
            ATTACHMENT_READ_SCHEMA,
        ),
        (
            "workspace_write",
            "Create or completely replace one workspace-relative text file after current-host-invocation workspace_list and workspace_read grounding. An existing target must itself have been read first. The result reports changed=false when the requested content already matches; move on instead of repeating that write.",
            r#"{"additionalProperties":false,"properties":{"content":{"type":"string"},"path":{"type":"string"}},"required":["content","path"],"type":"object"}"#,
        ),
        (
            "workspace_patch",
            "Replace an exact text fragment in one workspace-relative file after listing the workspace and reading this exact target in the current turn. By default the old fragment must occur exactly once.",
            r#"{"additionalProperties":false,"properties":{"new":{"type":"string"},"old":{"type":"string"},"path":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["new","old","path"],"type":"object"}"#,
        ),
        (
            "workspace_remove",
            "Remove one exact workspace-relative regular file after reading it, or one empty directory after listing it. Directory removal is non-recursive. Files that appear during the invocation through commands, services, hooks, or evaluators are external evidence and cannot be removed.",
            r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
        ),
        (
            "run_command",
            "Run a non-destructive structured executable and argv to completion through the harness-owned C4 router and C2 process lifecycle after current-host-invocation workspace_list and workspace_read grounding; use it to build, test, lint, inspect Git, apply caller-authorized external effects, and observe failures. Keep build/test worker counts at or below workspace_list.execution_resources.recommended_parallelism. The harness supplies cross-language concurrency defaults and rejects recognized explicit build fan-out above that observed ceiling with a retryable diagnostic. If a required executable is absent, verify its path and inspect available package or runtime managers; in an authorized disposable software or system task, install the ordinary prerequisite and retry the real command instead of fabricating a stand-in deliverable. Before inspecting a large binary, log, database, or generated file, prefer purpose-built filters, bounded ranges, or summary modes so only decision-relevant output enters model context. For binary, deleted, damaged, or truncated data, search for the strongest contract-supplied stable fragment and inspect a bounded neighboring byte or record window before speculative transforms or broad parameter searches; validate the reconstructed whole value against every declared constraint. When a command queries an API or parses structured data, print only the fields needed for the current decision; if the shape is unknown, begin with keys, counts, or a bounded sample instead of dumping nested metadata. Before transferring a whole remote repository, archive, or dataset, inspect an immutable manifest, index, tree, content length, or object-size summary and prefer targeted pinned records. After a bulk transfer times out, do not retry the same collection through a different bulk wrapper without new evidence that it fits the available command budget. Prefer relevant command output: the harness retains the complete structured request/result log for independent review, while the model-visible view may show a bounded excerpt with an explicit omission marker and digest. Use developer_evidence_read with offset pages to retrieve and verify the full retained log when reviewing command claims. Label each command as external_effect when it performs the requested action or verification when it freshly inspects the completed outcome. Commands default to a 120-second deadline; request any positive timeout representable by the millisecond process protocol for a known longer build or test. When an embedding caller explicitly selects a product deadline, each request is also clamped to that shared deadline while preserving a completion reserve; results report the requested timeout, actual allowance, remaining product seconds or null when unbounded, and whether the deadline limited the command. A timeout kills the owned process tree and returns captured output plus recovery guidance so the run can choose a materially bounded strategy. Harness-owned peritus-internal gates are unavailable here and run independently after the turn. Use workspace_remove for intentional file deletion.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"cwd":{"type":"string"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"timeout_seconds":{"default":120,"maximum":18446744073709551,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_start",
            "Start a structured command through the same C4/C2 ownership as run_command and return a stable handle immediately. Use interactive=true for programs that need terminal input; otherwise use false for a background pipe process. Follow with command_poll, command_stdin, command_resize, command_signal, command_cancel, or command_recover. The same grounding, resource, timeout, and non-destructive-command rules as run_command apply.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"columns":{"default":80,"maximum":65535,"minimum":1,"type":"integer"},"cwd":{"type":"string"},"interactive":{"default":true,"type":"boolean"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"rows":{"default":24,"maximum":65535,"minimum":1,"type":"integer"},"timeout_seconds":{"default":120,"maximum":18446744073709551,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_poll",
            "Poll a command_start handle. Returns bounded progress while running or the stable terminal result with captured output.",
            COMMAND_HANDLE_SCHEMA,
        ),
        (
            "command_stdin",
            "Write exact non-empty UTF-8 bytes to an active interactive command handle, then return its latest state. No newline is appended. Include a carriage return (\\r) to press terminal Enter.",
            r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"},"text":{"maxLength":65536,"minLength":1,"type":"string"}},"required":["handle","text"],"type":"object"}"#,
        ),
        (
            "command_resize",
            "Resize the terminal attached to an active interactive command handle.",
            r#"{"additionalProperties":false,"properties":{"columns":{"maximum":65535,"minimum":1,"type":"integer"},"handle":{"type":"string"},"rows":{"maximum":65535,"minimum":1,"type":"integer"}},"required":["columns","handle","rows"],"type":"object"}"#,
        ),
        (
            "command_signal",
            "Send a supported stable signal name to an active command handle, then return its latest state.",
            r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"},"signal":{"type":"string"}},"required":["handle","signal"],"type":"object"}"#,
        ),
        (
            "command_cancel",
            "Cancel an active command and its owned process tree, then return its latest state.",
            COMMAND_HANDLE_SCHEMA,
        ),
        (
            "command_recover",
            "Reconcile an active command handle with its durable C2 process state after an interrupted poll or control operation.",
            COMMAND_HANDLE_SCHEMA,
        ),
    ])?;
    if !has_selected_file_attachments {
        definitions.retain(|definition| definition.name().as_str() != "attachment_read");
    }
    Ok(definitions)
}

/// Returns the repository-inspection subset used by the mandatory design pass.
pub fn read_only_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path with current byte size and permission metadata. Pages stay within max_bytes; pass the full next_cursor object as cursor to continue the same path and depth. Check omissions and depth_limited before treating the listing as complete. The result reports the exact workspace_root and confirms that ordinary workspace paths are relative to it; remove that exact prefix once from absolute in-workspace paths. An exact user-named absolute directory outside the workspace is available only as read-only reference evidence.",
            WORKSPACE_LIST_SCHEMA,
        ),
        (
            "workspace_search",
            "Search for a literal string and return bounded matching excerpts with line and byte offsets. Pages stay within max_bytes; pass the full next_cursor object as cursor to continue the same path and query. Check omissions before treating the search as complete.",
            WORKSPACE_SEARCH_SCHEMA,
        ),
        (
            "workspace_read",
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file, or from an exact user-named absolute reference file outside the workspace. Pages report actual end_line and next_line_byte_offset so oversized lines can resume exactly. External references are read-only.",
            WORKSPACE_READ_SCHEMA,
        ),
        (
            "attachment_read",
            "Read a bounded page from the exact user-confirmed immutable attachment version and selected range listed in the governing input. Continue at next_offset; this never reopens a workspace path.",
            ATTACHMENT_READ_SCHEMA,
        ),
    ])
}

/// Adds the run-scoped evidence reader available only to an independent reviewer.
pub fn reviewer_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    let mut definitions = read_only_definitions()?;
    definitions.push(definition(
        "developer_evidence_read",
        "Read an exact UTF-8 byte page from an immutable section of the current independent-review evidence: transcript, diff, gates, developer_commands, prior, or correction. section defaults to developer_commands for compatibility. Use the section named by a prompt omission marker, begin at offset 0, and continue at next_offset until null. Each page includes the stable section name, total_bytes, and sha256 so the exact original source can be reconstructed and verified; historical transcript, diff, gates, and finding text are not recoverable from fresh workspace files.",
        REVIEW_EVIDENCE_READ_SCHEMA,
    )?);
    Ok(definitions)
}

pub fn in_place_definition() -> Result<ToolDefinition, ProductRunnerError> {
    definition(
        "workspace_scope",
        "Declare additional exact workspace-relative task files BEFORE a command creates or modifies them in an in-place folder. File reads/writes are enrolled automatically. This records comparison evidence, not permission; preserve unrelated/private files. Do not declare a whole home directory or build-cache tree.",
        r#"{"type":"object","additionalProperties":false,"properties":{"paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":256}},"required":["paths"]}"#,
    )
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
    let description = if matches!(name, "run_command" | "command_start") {
        format!(
            "{description} cwd must be a workspace-relative directory, such as in/project. Omit cwd or use . for the workspace root; absolute paths are rejected."
        )
    } else {
        description.to_owned()
    };
    Ok(ToolDefinition::new(
        ToolName::new(name.to_owned()).map_err(|error| protocol(&error))?,
        Some(BoundedText::new(description, limits).map_err(|error| protocol(&error))?),
        JsonSchema::parse(schema, SchemaDialect::Draft202012, JsonBounds::schema(limits))
            .map_err(|error| protocol(&error))?,
        // These portable schemas contain optional fields. Provider strict decoding is a
        // separate model-specific feature; the host still validates tool arguments and access.
        false,
    ))
}

fn protocol(error: &peritus_model_protocol::ProtocolError) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Provider,
        "construct developer tool catalog",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_reader_is_advertised_only_for_authenticated_file_inputs() {
        let without = definitions_for_attachments(false).expect("tool definitions");
        assert!(!without.iter().any(|tool| tool.name().as_str() == "attachment_read"));
        let with = definitions_for_attachments(true).expect("tool definitions");
        assert!(with.iter().any(|tool| tool.name().as_str() == "attachment_read"));
    }

    #[test]
    fn mutating_catalog_declares_fresh_grounding_protocol() {
        let tools = definitions().expect("tool definitions");
        let description = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.name().as_str() == name)
                .and_then(ToolDefinition::description)
                .map(BoundedText::expose_for_wire)
                .expect("tool description")
        };

        assert!(description("workspace_list").contains("Call this first"));
        assert!(description("workspace_list").contains("exact workspace_root"));
        assert!(description("workspace_list").contains("execution_resources"));
        assert!(description("workspace_list").contains("remove the exact root prefix once"));
        assert!(description("workspace_read").contains("exact current workspace target"));
        assert!(description("workspace_list").contains("read-only reference evidence"));
        assert!(description("workspace_read").contains("user-named reference evidence"));
        assert!(description("workspace_read").contains("does not ground workspace mutation"));
        assert!(description("workspace_patch").contains("in the current turn"));
        assert!(description("workspace_remove").contains("empty directory"));
        assert!(description("workspace_remove").contains("non-recursive"));
        assert!(description("run_command").contains("peritus-internal"));
        assert!(description("run_command").contains("120-second deadline"));
        assert!(description("run_command").contains("explicitly selects a product deadline"));
        assert!(description("run_command").contains("completion reserve"));
        assert!(description("run_command").contains("recommended_parallelism"));
        assert!(description("run_command").contains("cross-language concurrency defaults"));
        assert!(description("run_command").contains("decision-relevant output"));
        assert!(description("run_command").contains("strongest contract-supplied stable fragment"));
        assert!(description("run_command").contains("bounded neighboring byte or record window"));
        assert!(description("run_command").contains("queries an API"));
        assert!(description("run_command").contains("keys, counts"));
        assert!(description("run_command").contains("nested metadata"));
        assert!(description("run_command").contains("immutable manifest, index, tree"));
        assert!(description("run_command").contains("different bulk wrapper"));
        assert!(description("run_command").contains("complete structured request/result log"));
        assert!(
            description("run_command").contains("bounded excerpt with an explicit omission marker")
        );
        assert!(description("run_command").contains("developer_evidence_read with offset pages"));
        assert!(description("run_command").contains("recovery guidance"));
        assert!(description("run_command").contains("install the ordinary prerequisite"));
        assert!(description("run_command").contains("instead of fabricating a stand-in"));
        assert!(description("run_command").contains("external_effect"));
        assert!(description("run_command").contains("verification"));
        assert!(description("run_command").contains("C4 router and C2 process lifecycle"));
        assert!(description("command_start").contains("stable handle"));
        assert!(description("command_start").contains("command_recover"));
        for name in ["run_command", "command_start"] {
            assert!(description(name).contains("cwd must be a workspace-relative directory"));
            assert!(description(name).contains("Omit cwd or use ."));
        }
        for name in [
            "command_poll",
            "command_stdin",
            "command_resize",
            "command_signal",
            "command_cancel",
            "command_recover",
        ] {
            assert!(tools.iter().any(|tool| tool.name().as_str() == name), "missing {name}");
        }
    }
}
