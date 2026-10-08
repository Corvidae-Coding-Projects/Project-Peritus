//! Enroll exact paths before effects so the shared pipeline can qualify non-Git work.

use super::{WorkspaceDeveloperTools, WorkspaceToolMode};
use crate::developer_tools::{
    argument_contract,
    command_runtime::CommandExecutionMode,
    path::tool,
    wire::{object, required_string},
};
use peritus_agent::DeveloperLoopError;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

impl WorkspaceDeveloperTools {
    pub(super) fn prepare_in_place(
        &self,
        name: &str,
        arguments: &Value,
        process_mode: Option<CommandExecutionMode>,
    ) -> Result<(), DeveloperLoopError> {
        let Some(scope) = &self.in_place_scope else {
            return Ok(());
        };
        // Read-only design/review must not enroll new paths or change candidate identity.
        if self.mode == WorkspaceToolMode::ReadOnly {
            return Ok(());
        }
        if matches!(
            name,
            "workspace_read" | "workspace_write" | "workspace_patch" | "workspace_remove"
        ) {
            scope
                .enroll(required_string(arguments, "path")?)
                .map_err(|error| tool(error.to_string()))?;
        }
        if matches!(name, "run_command" | "command_start")
            && process_mode == Some(CommandExecutionMode::Mutation)
            && scope.paths().map_err(|error| tool(error.to_string()))?.is_empty()
        {
            return Err(tool(
                "Declare exact task files with workspace_scope before commands in an in-place folder; no whole-folder inventory is taken",
            ));
        }
        Ok(())
    }

    pub(super) fn declare_in_place(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        if self.mode == WorkspaceToolMode::ReadOnly {
            return Err(tool("this role has read-only workspace access"));
        }
        let scope = self
            .in_place_scope
            .as_ref()
            .ok_or_else(|| tool("workspace_scope is only available for in-place delivery"))?;
        let paths = arguments
            .get("paths")
            .and_then(Value::as_array)
            .filter(|paths| !paths.is_empty())
            .ok_or_else(|| tool("workspace_scope requires a non-empty physical page of exact file paths"))?;
        let mut checked = Vec::with_capacity(paths.len());
        for path in paths {
            let path = path.as_str().ok_or_else(|| tool("scope path must be text"))?;
            self.access_policy
                .authorize("workspace_read", &serde_json::json!({"path":path}))
                .map_err(tool)?;
            checked.push(path);
        }
        let mut accepted = Vec::with_capacity(checked.len());
        for (index, path) in checked.iter().enumerate() {
            if let Err(error) = scope.enroll(path) {
                return Ok(scope_page_result(
                    scope,
                    &accepted,
                    Some((index, path, error.to_string())),
                )?);
            }
            accepted.push(*path);
        }
        scope_page_result(scope, &accepted, None)
    }
}

fn scope_page_result(
    scope: &crate::workspace_delivery::scope::ScopedBaseline,
    accepted: &[&str],
    failure: Option<(usize, &str, String)>,
) -> Result<Value, DeveloperLoopError> {
    let tracked = scope.paths().map_err(|error| tool(error.to_string()))?;
    let mut hasher = Sha256::new();
    for path in accepted {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
    }
    let accepted_paths = accepted
        .iter()
        .map(|path| Value::String((*path).to_owned()))
        .collect();
    let (success, failure_kind, failed_index, failed_path, failure_detail, next_path_index) =
        failure.map_or(
            (true, Value::Null, Value::Null, Value::Null, Value::Null, Value::Null),
            |(index, path, detail)| {
                let kind = if detail.contains("exact file or empty-directory")
                    || detail.contains("nonempty directory")
                    || detail.contains("protected")
                    || detail.contains("outside")
                {
                    "malformed_input"
                } else if detail.contains("changed while") {
                    "temporary_source_change"
                } else {
                    "temporary_capacity"
                };
                (
                    false,
                    Value::String(kind.to_owned()),
                    Value::from(index),
                    Value::String(path.to_owned()),
                    Value::String(detail),
                    Value::from(index),
                )
            },
        );
    Ok(object(vec![
        ("accepted_path_count", Value::from(accepted.len())),
        ("accepted_paths_sha256", Value::String(hex(hasher.finalize().into()))),
        ("contract_version", Value::String(argument_contract::VERSION.to_owned())),
        ("failed_index", failed_index),
        ("failed_path", failed_path),
        ("failure_detail", failure_detail),
        ("failure_kind", failure_kind),
        ("next_path_index", next_path_index),
        ("page_complete", Value::Bool(success)),
        ("success", Value::Bool(success)),
        ("tracked_path_count", Value::from(tracked.len())),
        // Retain the legacy field as this accepted physical page, rather than an ever-growing
        // cumulative projection that can itself become impossible to represent.
        ("tracked_paths", Value::Array(accepted_paths)),
    ]))
}

fn hex(bytes: [u8; 32]) -> String {
    use core::fmt::Write as _;

    bytes.iter().fold(String::with_capacity(64), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}
