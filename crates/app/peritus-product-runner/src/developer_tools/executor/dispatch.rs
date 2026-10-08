//! Proposal validation and effect dispatch, shared by synchronous and awaitable execution.

use super::{WorkspaceDeveloperTools, WorkspaceToolMode};
use crate::developer_tools::{
    arguments,
    path::tool,
    wire::{object, observation},
};
use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;

pub(super) enum PreparedTool {
    Ready { arguments: Value, effect: bool },
    Observed(DeveloperToolObservation),
}

impl WorkspaceDeveloperTools {
    fn denied(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        detail: impl Into<String>,
    ) -> Result<PreparedTool, DeveloperLoopError> {
        let value = object(vec![("error", Value::String(detail.into()))]);
        self.finish_observation(call, arguments, &value, true, false, false)
            .map(PreparedTool::Observed)
    }

    pub(super) fn prepare_dispatch(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<PreparedTool, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        if let Some(detail) = self.request_source_denial(call.name().as_str()) {
            return self.denied(call, &arguments, detail);
        }
        // No enrollment, checkpoint, receipt or effect may precede live permission validation.
        if let Some(detail) = self.permission_denial(call.name().as_str()) {
            return self.denied(call, &arguments, detail);
        }
        self.refresh_hard_constraints();
        if let Err(detail) = self.access_policy.authorize(call.name().as_str(), &arguments) {
            return self.denied(call, &arguments, detail);
        }
        if self.mode == WorkspaceToolMode::ReadOnly
            && !matches!(
                call.name().as_str(),
                "request_sources" | "request_source_read"
                    | "context_sources" | "context_source_read"
                    | "workspace_list" | "workspace_search" | "workspace_read"
            )
        {
            return self.denied(call, &arguments, "this role has read-only workspace access");
        }
        if let Err(error) = arguments::validate(call.name().as_str(), &arguments) {
            return self.denied(call, &arguments, error.to_string());
        }
        let effect = matches!(
            call.name().as_str(),
            "workspace_write"
                | "workspace_patch"
                | "workspace_remove"
                | "run_command"
                | "command_start"
                | "command_stdin"
                | "command_resize"
                | "command_signal"
                | "command_cancel"
        ) && self.mode == WorkspaceToolMode::ReadWrite;
        if effect && let Some(observation) = self.replay_effect(call, &arguments)? {
            return Ok(PreparedTool::Observed(observation));
        }
        if let Err(error) = self.prepare_in_place(call.name().as_str(), &arguments) {
            return self.denied(call, &arguments, error.to_string());
        }
        Ok(PreparedTool::Ready { arguments, effect })
    }

    pub(super) fn dispatch_prepared(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        effect: bool,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if effect && let Some(observation) = self.begin_effect(call, arguments)? {
            return Ok(observation);
        }
        let dispatched = self.dispatch_tool(call, arguments);
        self.finish_dispatched(call, arguments, effect, dispatched)
    }

    pub(super) async fn dispatch_prepared_async(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        effect: bool,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if effect && let Some(observation) = self.begin_effect(call, arguments)? {
            return Ok(observation);
        }
        let dispatched = match call.name().as_str() {
            "run_command" => {
                self.run_command_async(arguments, call.id().expose_for_wire()).await
            }
            "command_start" => {
                self.start_command_async(arguments, call.id().expose_for_wire()).await
            }
            "command_poll" => self.poll_command_async(arguments).await,
            "command_stdin" => self.write_command_stdin_async(arguments).await,
            "command_resize" => self.resize_command_async(arguments).await,
            "command_signal" => self.signal_command_async(arguments).await,
            "command_cancel" => self.cancel_command_async(arguments).await,
            "command_recover" => self.recover_command_async(arguments).await,
            _ => self.dispatch_tool(call, arguments),
        };
        self.finish_dispatched(call, arguments, effect, dispatched)
    }

    fn finish_dispatched(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        effect: bool,
        dispatched: Result<Value, DeveloperLoopError>,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let (mut value, is_error, accepted) = match dispatched {
            Ok(value) => {
                let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                (value, is_error, true)
            }
            Err(DeveloperLoopError::Cancelled) => return Err(DeveloperLoopError::Cancelled),
            Err(error) => (object(vec![("error", Value::String(error.to_string()))]), true, false),
        };
        if accepted {
            if let Some((handle, request)) = self.active_commands.pending_checkpoint(&value) {
                // Finalize the original command's checkpoint obligation even when this receipt
                // came from a read-only poll/recovery. No process effect is dispatched here.
                self.record_checkpoint("command_poll", &request, &value)?;
                self.active_commands.checkpoint_completed(&handle)?;
            }
            self.active_commands.observe(
                &self.root,
                &mut value,
                &mut self.ownership,
                &mut self.command_evidence,
            )?;
        }
        if effect {
            if accepted {
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .applied(&value, is_error)?;
                self.record_checkpoint(call.name().as_str(), arguments, &value)?;
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .finalize()?;
            } else {
                self.receipts
                    .as_mut()
                    .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                    .complete(&value, is_error)?;
            }
        }
        self.finish_observation(call, arguments, &value, is_error, accepted, effect && accepted)
    }
}
