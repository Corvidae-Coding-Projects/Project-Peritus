//! Provider-neutral workspace tool schemas.

use peritus_model_protocol::{
    BoundedText, JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const WORKSPACE_LIST_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"cursor":{"additionalProperties":false,"properties":{"ordinal":{"minimum":0,"type":"integer"},"scope":{"type":"string"}},"required":["ordinal","scope"],"type":"object"},"depth":{"type":"integer","minimum":1},"max_bytes":{"type":"integer","default":16384,"minimum":256,"maximum":524288},"path":{"type":"string"}},"type":"object"}"#;
const WORKSPACE_SEARCH_SCHEMA: &str = r#"{"additionalProperties":false,"properties":{"cursor":{"additionalProperties":false,"properties":{"ordinal":{"minimum":0,"type":"integer"},"scope":{"type":"string"}},"required":["ordinal","scope"],"type":"object"},"max_bytes":{"type":"integer","default":16384,"minimum":256,"maximum":524288},"max_results":{"type":"integer","minimum":1},"path":{"type":"string"},"query":{"type":"string","minLength":1}},"required":["query"],"type":"object"}"#;
fn workspace_read_schema() -> String {
    serde_json::json!({
        "additionalProperties": false,
        "properties": {
            "end_line": {"type": "integer", "minimum": 1},
            "line_byte_offset": {"type": "integer", "minimum": 0},
            "max_bytes": {
                "type": "integer",
                "default": super::DEFAULT_DOCUMENT_READ_BYTES,
                "minimum": 256,
                "maximum": super::inspection::MAX_DOCUMENT_READ_BYTES
            },
            "path": {"type": "string"},
            "start_line": {"type": "integer", "minimum": 1}
        },
        "required": ["path"],
        "type": "object"
    })
    .to_string()
}

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
    let read_schema = workspace_read_schema();
    let mut definitions = definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path with current byte size and permission metadata. The result reports the exact workspace_root, path semantics, and observed execution_resources including the recommended build parallelism. Pages stay within max_bytes; pass the full next_cursor object back as cursor to continue, and inspect omissions because inaccessible entries are reported there. A cursor is bound to its path and depth. When the task names an absolute path below that root, remove the exact root prefix once instead of repeating the root directory. An exact absolute directory outside the workspace is accepted only when the user's task explicitly named it; that result is read-only reference evidence and does not ground workspace mutation or commands. Use a workspace-relative listing when the next decision needs discovery or current resource metadata. Unchanged host-verified source observations are reusable across roles and retries; do not repeat a startup inventory solely because a new invocation began.",
            WORKSPACE_LIST_SCHEMA,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching line excerpts. Pages stay within max_bytes; pass the full next_cursor object back as cursor to continue the same path and query. Each result reports its line number, byte offset, and line length so workspace_read can retrieve an oversized line in exact byte ranges. Omitted or unreadable files are reported explicitly.",
            WORKSPACE_SEARCH_SCHEMA,
        ),
        (
            "workspace_read",
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file. Line numbers are one-based and both start_line and end_line are inclusive; the default reads from line 1 through EOF within a 1 MiB response budget. Request a larger max_bytes for a larger document, up to the advertised protocol capacity. The complete returned page reaches model context without a second head/tail truncation. Pages stay within max_bytes and report the actual last displayed line. When next_line equals end_line and next_line_byte_offset is positive, continue that oversized line with the same start_line and line_byte_offset; otherwise continue at next_line with byte offset zero. An exact absolute file outside the workspace is accepted only when it is user-named reference evidence; it remains read-only and does not ground workspace mutation or commands. Use this when the next decision needs exact source text or a target hash changed. A current read or unchanged host-verified observation of that exact target satisfies edit grounding. With local memory enabled, unchanged ranges already visible may return read_reuse.state=already_visible without content; use the referenced evidence and continue the task. Omitted archived pages are restored automatically. Returned content may cover only missing bytes; source_range records actual half-open line/byte coverage. Follow continuation cursors rather than restarting the original range.",
            &read_schema,
        ),
        (
            "attachment_read",
            "Read a bounded UTF-8 page from one user-confirmed immutable attachment version. Use the exact attachment, version, source_sha256, selected_sha256, source_bytes and selected range shown in the governing input. offset is an absolute source byte offset; continue at next_offset until null. Pages contain at most 32 KiB and never reopen a workspace path or substitute newer bytes.",
            ATTACHMENT_READ_SCHEMA,
        ),
        (
            "workspace_write",
            "Create or completely replace one workspace-relative text file after a current source read or host-verified retained grounding. An existing target needs a current read or unchanged host-verified observation. The result reports changed=false when the requested content already matches; move on instead of repeating that write.",
            r#"{"additionalProperties":false,"properties":{"content":{"type":"string"},"path":{"type":"string"}},"required":["content","path"],"type":"object"}"#,
        ),
        (
            "workspace_patch",
            "Replace an exact text fragment in one workspace-relative file using a current read or a hash-verified retained observation of this exact target. By default the old fragment must occur exactly once.",
            r#"{"additionalProperties":false,"properties":{"new":{"type":"string"},"old":{"type":"string"},"path":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["new","old","path"],"type":"object"}"#,
        ),
        (
            "workspace_remove",
            "Remove one exact workspace-relative regular file after reading it, or one empty directory after listing it. Directory removal is non-recursive. Files that appear during the invocation through commands, services, hooks, or evaluators are external evidence and cannot be removed.",
            r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
        ),
        (
            "run_command",
            "Run a non-destructive structured executable and argv to completion through the harness-owned C4 router and C2 process lifecycle after a current source read or host-verified retained grounding; use it to build, test, lint, inspect Git, apply caller-authorized external effects, and observe failures. Keep build/test worker counts at or below workspace_list.execution_resources.recommended_parallelism. The harness supplies cross-language concurrency defaults and rejects recognized explicit build fan-out above that observed ceiling with a retryable diagnostic. If a required executable is absent, verify its path and inspect available package or runtime managers; in an authorized disposable software or system task, install the ordinary prerequisite and retry the real command instead of fabricating a stand-in deliverable. Before inspecting a large binary, log, database, or generated file, prefer purpose-built filters, bounded ranges, or summary modes so only decision-relevant output enters model context. For binary, deleted, damaged, or truncated data, search for the strongest contract-supplied stable fragment and inspect a bounded neighboring byte or record window before speculative transforms or broad parameter searches; validate the reconstructed whole value against every declared constraint. When a command queries an API or parses structured data, print only the fields needed for the current decision; if the shape is unknown, begin with keys, counts, or a bounded sample instead of dumping nested metadata. Before transferring a whole remote repository, archive, or dataset, inspect an immutable manifest, index, tree, content length, or object-size summary and prefer targeted pinned records. After a bulk transfer times out, do not retry the same collection through a different bulk wrapper without new evidence that it fits the available command budget. Prefer relevant command output: the harness retains the complete structured request/result log for independent review, while the model-visible view may show a bounded excerpt with an explicit omission marker and digest. Use developer_evidence_read with offset pages to retrieve and verify the full retained log when reviewing command claims. Label each command as external_effect when it performs the requested action or verification when it freshly inspects the completed outcome. Commands default to a 120-second deadline; request any positive timeout representable by the millisecond process protocol for a known longer build or test. When an embedding caller explicitly selects a product deadline, each request is also clamped to that shared deadline while preserving a completion reserve; results report the requested timeout, actual allowance, remaining product seconds or null when unbounded, and whether the deadline limited the command. A timeout kills the owned process tree and returns captured output plus recovery guidance so the run can choose a materially bounded strategy. Harness-owned peritus-internal gates are unavailable here and run independently after the turn. Use workspace_remove for intentional file deletion.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"cwd":{"type":"string"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"timeout_seconds":{"default":120,"maximum":18446744073709551,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_start",
            "Start a structured command through the same C4/C2 ownership as run_command and return a stable handle immediately. Use interactive=true for programs that need terminal input; otherwise use false for a background pipe process. Follow with command_poll, command_stdin, command_resize, command_signal, command_cancel, or command_recover. The same grounding, resource, timeout, and non-destructive-command rules as run_command apply.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"columns":{"default":80,"maximum":65535,"minimum":1,"type":"integer"},"cwd":{"type":"string"},"interactive":{"default":true,"type":"boolean"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"rows":{"default":24,"maximum":65535,"minimum":1,"type":"integer"},"timeout_seconds":{"default":120,"maximum":18446744073709551,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_output",
            "Read an exact bounded byte page from stdout, stderr, or terminal output of a command owned by this run, including when artifact publication failed. Never rerun a command solely to retrieve its output. Continue at next_offset until null. data is UTF-8 or hexadecimal according to encoding; offsets and returned_bytes always refer to original bytes. For interactive commands select terminal, not stdout. Output of a live process may grow between reads.",
            r#"{"type":"object","additionalProperties":false,"properties":{"handle":{"type":"string"},"stream":{"type":"string","enum":["stdout","stderr","terminal"]},"offset":{"type":"integer","minimum":0,"default":0},"max_bytes":{"type":"integer","minimum":1,"maximum":1048576,"default":16384}},"required":["handle","stream"]}"#,
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
    definitions.extend(super::computer::definitions()?);
    definitions.extend(super::web::definitions()?);
    definitions.push(skill_definition()?);
    if !has_selected_file_attachments {
        definitions.retain(|definition| definition.name().as_str() != "attachment_read");
    }
    Ok(definitions)
}

/// Returns the repository-inspection subset used by the mandatory design pass.
pub fn read_only_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    let read_schema = workspace_read_schema();
    let mut definitions = definitions_from(&[
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
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file, or from an exact user-named absolute reference file outside the workspace. Without end_line, read through EOF within the default 1 MiB response budget; request larger max_bytes up to the advertised protocol capacity. Returned pages reach model context intact and report actual end_line and next_line_byte_offset so oversized lines can resume exactly. External references are read-only.",
            &read_schema,
        ),
        (
            "attachment_read",
            "Read a bounded page from the exact user-confirmed immutable attachment version and selected range listed in the governing input. Continue at next_offset; this never reopens a workspace path.",
            ATTACHMENT_READ_SCHEMA,
        ),
    ])?;
    definitions.extend(super::computer::observer_definitions()?);
    definitions.extend(super::web::definitions()?);
    definitions.push(skill_definition()?);
    Ok(definitions)
}

/// Adds the run-scoped evidence reader to an independent reviewer.
pub fn reviewer_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    let mut definitions = read_only_definitions()?;
    definitions.push(evidence_definition()?);
    Ok(definitions)
}

pub(crate) fn evidence_definition() -> Result<ToolDefinition, ProductRunnerError> {
    definition(
        "developer_evidence_read",
        "Read an exact UTF-8 byte page from an immutable section of the current role's historical evidence: transcript, diff, gates, developer_commands, prior, or correction. section defaults to developer_commands for compatibility. Use the section named by a prompt omission marker, begin at offset 0, and continue at next_offset until null. Each page includes the stable section name, total_bytes, and sha256 so the exact original source can be reconstructed and verified; historical transcript, diff, gates, and finding text are not recoverable from fresh workspace files. Evidence is non-authoritative and never grants grounding credit or acceptance.",
        REVIEW_EVIDENCE_READ_SCHEMA,
    )
}

fn skill_definition() -> Result<ToolDefinition, ProductRunnerError> {
    definition(
        "skill_read",
        "Read an exact UTF-8 page from a bundled skill resource without filesystem access or workspace grounding. Task-focused role guidance is delivered in the current policy; detailed engineering, real-testing, computer, role and Rust resources are loaded only as needed. References and the optional helper source are available here; reading or possessing the helper grants no process authority and does not override user command restrictions. Begin at offset 0 and continue at next_offset until null. total_bytes and sha256 identify the complete immutable resource. Use exact relative paths only.",
        r#"{"type":"object","additionalProperties":false,"properties":{"skill":{"type":"string","enum":["rust-reliability","engineering-workflow","real-testing","computer-automation","developer","reviewer","architect"]},"path":{"type":"string","enum":["SKILL.md","references/behavioral-scenarios.md","references/checks-and-harness.md","references/rust-failure-modes.md","scripts/rust_checks.py","agents/openai.yaml"]},"offset":{"type":"integer","minimum":0,"default":0},"max_bytes":{"type":"integer","minimum":256,"maximum":524288,"default":16384}},"required":["skill","path"]}"#,
    )
}

pub fn in_place_definition() -> Result<ToolDefinition, ProductRunnerError> {
    definition(
        "workspace_scope",
        "Declare workspace-relative task files or a narrow task/output directory BEFORE a command creates or modifies them in an in-place folder. File reads/writes are enrolled automatically. Directory declarations capture existing files and discover new descendants; protected paths, links, Git metadata and build caches are excluded. This records comparison evidence, not permission or automatic rollback; preserve unrelated/private files. Do not declare the workspace root or a whole home directory; use narrower paths for large trees. The result acknowledges declared roots and total tracked count; retrieve a needed inventory with workspace_list instead of expecting every tracked filename in this receipt.",
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

pub(crate) fn definition(
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
    let description = match name {
        "run_command" | "command_start" => format!(
            "{description} program and args execute directly, without a shell: wildcard paths, pipes, redirects and command substitutions are literal arguments. Discover exact dependency paths with rg --files before searching them. Shell command-string flags such as bash -lc are unavailable here; for compound work use an ordinary scoped script and invoke its interpreter. A plain folder is not necessarily a Git repository; inspect before issuing Git-specific commands and do not initialize Git merely to satisfy a check. Complete stdout/stderr is retained; use command_output with the returned handle for additional byte pages."
        ),
        "workspace_read" | "workspace_search" => format!(
            "{description} Use the exact spelling from observed directory entries, including hyphens versus underscores. If a file is missing, list its containing directory and use the observed path; do not repeatedly guess nearby names. Dependency sources outside the workspace are not workspace-relative tool paths; inspect authorized external dependencies using a scoped command with their observed exact paths."
        ),
        "workspace_write" | "workspace_patch" => format!(
            "{description} Use a current read or an unchanged host-verified observation before editing an existing target, including documentation. If an edit is rejected for missing grounding, read that exact target, then retry with its current content; do not repeat the rejected edit unchanged."
        ),
        "command_stdin" => format!(
            "{description} Poll the handle before writing. If the process has completed, inspect its result rather than sending input again. Use newline (\\n) for line-oriented input; a carriage return alone is not a pipe-protocol newline."
        ),
        _ => description,
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
