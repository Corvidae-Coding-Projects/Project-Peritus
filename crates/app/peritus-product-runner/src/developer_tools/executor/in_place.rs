//! Enroll task files or bounded directory subtrees before effects in non-Git work.

use super::{WorkspaceDeveloperTools, WorkspaceToolMode};
use crate::developer_tools::{
    path::tool,
    wire::{object, required_string},
};
use peritus_agent::DeveloperLoopError;
use serde_json::Value;

impl WorkspaceDeveloperTools {
    pub(super) fn prepare_in_place(
        &self,
        name: &str,
        arguments: &Value,
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
            && scope.paths().map_err(|error| tool(error.to_string()))?.is_empty()
        {
            return Err(tool(
                "Declare task files or a scoped output directory with workspace_scope before commands in an in-place folder; unrelated folders are not inventoried",
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
            .filter(|paths| !paths.is_empty() && paths.len() <= 256)
            .ok_or_else(|| tool("workspace_scope requires 1..256 task file or directory paths"))?;
        for path in paths {
            let path = path.as_str().ok_or_else(|| tool("scope path must be text"))?;
            self.access_policy
                .authorize("workspace_read", &serde_json::json!({"path":path}))
                .map_err(tool)?;
            scope.enroll(path).map_err(|error| tool(error.to_string()))?;
        }
        let tracked = scope.paths().map_err(|error| tool(error.to_string()))?;
        Ok(object(vec![
            ("declared_paths", Value::Array(paths.clone())),
            ("tracked_path_count", Value::from(tracked.len())),
            ("inventory_included", Value::Bool(false)),
            ("note", Value::String("Scope retained. Use workspace_list on a specific directory when its inventory is needed; unchanged tracked paths are not repeated here.".to_owned())),
        ]))
    }
}
