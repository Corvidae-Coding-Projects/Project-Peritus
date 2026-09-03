//! Candidate-checkpoint classification for accepted developer tool effects.

use std::sync::Arc;

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{path::tool, wire::string};

/// Material tool boundary that requires an exact candidate checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCheckpointBoundary {
    /// Workspace bytes changed.
    Mutation,
    /// A declared verification command completed successfully.
    Verification,
    /// A caller-authorized external effect completed successfully.
    ExternalEffect,
}

pub(super) type ToolCheckpointObserver =
    Arc<dyn Fn(ToolCheckpointBoundary) -> Result<(), String> + Send + Sync>;

impl WorkspaceDeveloperTools {
    pub(super) fn record_checkpoint(
        &self,
        name: &str,
        arguments: &Value,
        result: &Value,
    ) -> Result<(), DeveloperLoopError> {
        let boundary = match name {
            "workspace_write" if result.get("changed").and_then(Value::as_bool) == Some(true) => {
                Some(ToolCheckpointBoundary::Mutation)
            }
            "workspace_patch" | "workspace_remove" if result.get("error").is_none() => {
                Some(ToolCheckpointBoundary::Mutation)
            }
            "run_command" | "command_poll" | "command_recover"
                if result.get("success").and_then(Value::as_bool) == Some(true)
                    && (name == "run_command"
                        || result.get("state").and_then(Value::as_str) == Some("completed")) =>
            {
                match string(arguments, "purpose")
                    .or_else(|| result.get("purpose").and_then(Value::as_str))
                {
                    Some("verification") => Some(ToolCheckpointBoundary::Verification),
                    Some("external_effect") => Some(ToolCheckpointBoundary::ExternalEffect),
                    _ => None,
                }
            }
            _ => None,
        };
        if let (Some(observer), Some(boundary)) = (&self.checkpoint_observer, boundary) {
            observer(boundary).map_err(tool)?;
        }
        Ok(())
    }
}
