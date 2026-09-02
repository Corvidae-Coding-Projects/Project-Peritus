//! Provider-neutral workspace tool schemas.

use peritus_model_protocol::{
    BoundedText, JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolDefinition, ToolName,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub fn definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path with current byte size and permission metadata. The result reports the exact workspace_root, path semantics, and observed execution_resources including the recommended build parallelism. When the task names an absolute path below that root, remove the exact root prefix once instead of repeating the root directory. Call this first in every fresh writer or fixer turn; mutation and process tools remain locked until a successful listing and a targeted file read.",
            r#"{"additionalProperties":false,"properties":{"depth":{"type":"integer"},"path":{"type":"string"}},"type":"object"}"#,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching lines.",
            r#"{"additionalProperties":false,"properties":{"max_results":{"type":"integer"},"path":{"type":"string"},"query":{"type":"string"}},"required":["query"],"type":"object"}"#,
        ),
        (
            "workspace_read",
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file. Call this after workspace_list and read the exact current target before changing an existing file.",
            r#"{"additionalProperties":false,"properties":{"end_line":{"type":"integer"},"path":{"type":"string"},"start_line":{"type":"integer"}},"required":["path"],"type":"object"}"#,
        ),
        (
            "workspace_write",
            "Create or completely replace one workspace-relative text file after current-turn workspace_list and workspace_read grounding. An existing target must itself have been read first. The result reports changed=false when the requested content already matches; move on instead of repeating that write.",
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
            "Run a non-destructive structured executable and argv to completion through the harness-owned C4 router and C2 process lifecycle after current-turn workspace_list and workspace_read grounding; use it to build, test, lint, inspect Git, apply caller-authorized external effects, and observe failures. Keep build/test worker counts at or below workspace_list.execution_resources.recommended_parallelism. The harness supplies cross-language concurrency defaults and rejects recognized explicit build fan-out above that observed ceiling with a retryable diagnostic. If a required executable is absent, verify its path and inspect available package or runtime managers; in an authorized disposable software or system task, install the ordinary prerequisite and retry the real command instead of fabricating a stand-in deliverable. Before inspecting a large binary, log, database, or generated file, prefer purpose-built filters, bounded ranges, or summary modes so only decision-relevant output enters model context. For binary, deleted, damaged, or truncated data, search for the strongest contract-supplied stable fragment and inspect a bounded neighboring byte or record window before speculative transforms or broad parameter searches; validate the reconstructed whole value against every declared constraint. When a command queries an API or parses structured data, print only the fields needed for the current decision; if the shape is unknown, begin with keys, counts, or a bounded sample instead of dumping nested metadata. Before transferring a whole remote repository, archive, or dataset, inspect an immutable manifest, index, tree, content length, or object-size summary and prefer targeted pinned records. After a bulk transfer times out, do not retry the same collection through a different bulk wrapper without new evidence that it fits the available command budget. The hard output cap is a fallback, not a target. When output still exceeds that cap, the result preserves both its opening context and final diagnostics while omitting the noisy middle. Label each command as external_effect when it performs the requested action or verification when it freshly inspects the completed outcome. Commands default to a 120-second deadline; request 1 through 600 seconds for a known longer build or test. Each request is also clamped to the live shared product deadline while preserving a completion reserve; results report the requested timeout, actual allowance, remaining product seconds, and whether the deadline limited the command. A timeout kills the owned process tree and returns captured output plus recovery guidance so the run can choose a materially bounded strategy. Harness-owned peritus-internal gates are unavailable here and run independently after the turn. Use workspace_remove for intentional file deletion.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"cwd":{"type":"string"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"timeout_seconds":{"default":120,"maximum":600,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_start",
            "Start a structured command through the same C4/C2 ownership as run_command and return a stable handle immediately. Use interactive=true for programs that need terminal input; otherwise use false for a background pipe process. Follow with command_poll, command_stdin, command_resize, command_signal, command_cancel, or command_recover. The same grounding, resource, timeout, and non-destructive-command rules as run_command apply.",
            r#"{"additionalProperties":false,"properties":{"args":{"items":{"type":"string"},"type":"array"},"columns":{"default":80,"maximum":65535,"minimum":1,"type":"integer"},"cwd":{"type":"string"},"interactive":{"default":true,"type":"boolean"},"program":{"type":"string"},"purpose":{"enum":["external_effect","verification"],"type":"string"},"rows":{"default":24,"maximum":65535,"minimum":1,"type":"integer"},"timeout_seconds":{"default":120,"maximum":600,"minimum":1,"type":"integer"}},"required":["args","program","purpose"],"type":"object"}"#,
        ),
        (
            "command_poll",
            "Poll a command_start handle. Returns bounded progress while running or the stable terminal result with captured output.",
            r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"}},"required":["handle"],"type":"object"}"#,
        ),
        (
            "command_stdin",
            "Write non-empty UTF-8 text to an active interactive command handle, then return its latest state.",
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
            r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"}},"required":["handle"],"type":"object"}"#,
        ),
        (
            "command_recover",
            "Reconcile an active command handle with its durable C2 process state after an interrupted poll or control operation.",
            r#"{"additionalProperties":false,"properties":{"handle":{"type":"string"}},"required":["handle"],"type":"object"}"#,
        ),
    ])
}

/// Returns the repository-inspection subset used by the mandatory design pass.
pub fn read_only_definitions() -> Result<Vec<ToolDefinition>, ProductRunnerError> {
    definitions_from(&[
        (
            "workspace_list",
            "List files and directories below one workspace-relative path with current byte size and permission metadata. The result reports the exact workspace_root and confirms that every workspace tool path is relative to it; remove that exact prefix once from absolute in-workspace paths.",
            r#"{"additionalProperties":false,"properties":{"depth":{"type":"integer"},"path":{"type":"string"}},"type":"object"}"#,
        ),
        (
            "workspace_search",
            "Search text files for a literal string and return matching lines.",
            r#"{"additionalProperties":false,"properties":{"max_results":{"type":"integer"},"path":{"type":"string"},"query":{"type":"string"}},"required":["query"],"type":"object"}"#,
        ),
        (
            "workspace_read",
            "Read a bounded line range plus current byte size and permission metadata from one workspace-relative text file.",
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(description("workspace_read").contains("exact current target"));
        assert!(description("workspace_patch").contains("in the current turn"));
        assert!(description("workspace_remove").contains("empty directory"));
        assert!(description("workspace_remove").contains("non-recursive"));
        assert!(description("run_command").contains("peritus-internal"));
        assert!(description("run_command").contains("120-second deadline"));
        assert!(description("run_command").contains("live shared product deadline"));
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
        assert!(description("run_command").contains("hard output cap is a fallback"));
        assert!(description("run_command").contains("final diagnostics"));
        assert!(description("run_command").contains("recovery guidance"));
        assert!(description("run_command").contains("install the ordinary prerequisite"));
        assert!(description("run_command").contains("instead of fabricating a stand-in"));
        assert!(description("run_command").contains("external_effect"));
        assert!(description("run_command").contains("verification"));
        assert!(description("run_command").contains("C4 router and C2 process lifecycle"));
        assert!(description("command_start").contains("stable handle"));
        assert!(description("command_start").contains("command_recover"));
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
