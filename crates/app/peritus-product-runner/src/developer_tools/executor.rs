//! Concrete bounded filesystem and structured-command developer tools.

use std::{fs, path::PathBuf};

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::CompletedToolCall;
use serde_json::Value;

use super::{
    access_policy::WorkspaceAccessPolicy,
    command_budget::CommandBudget,
    effect::atomic_write,
    evidence::CommandEvidence,
    grounding::GroundingEvidence,
    inspection,
    ownership::WorkspaceOwnership,
    path::{checked, tool},
    receipt::{EffectReceiptLedger, ReceiptDecision},
    removal,
    resources::CommandResources,
    wire::{object, observation, required_string, string},
};
use crate::control::{HostPermissions, PermissionCapability};
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const TOOLS_WITHOUT_DELIVERY_PROGRESS: u16 = 12;
const MAX_PROGRESS_NUDGES: u8 = 2;
const PROGRESS_FEEDBACK: &str = "The harness observed a long inspection sequence without a workspace mutation or successful declared external effect. Choose the shortest concrete delivery step now. If a standard capability is missing and the active disposable task authorizes installation, use the available package or runtime manager before hand-writing a substitute. Otherwise write or apply the requested result, then verify it. Continue inspecting only when a specific unresolved requirement still needs evidence.";

mod active;
mod checkpoint_observer;
mod command;
mod construction;
mod in_place;
mod inspection_progress;

use active::ActiveCommandLedger;
use checkpoint_observer::PreparedMutation;
pub use checkpoint_observer::ToolCheckpointBoundary;
use checkpoint_observer::ToolCheckpointObserver;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkspaceToolMode {
    ReadWrite,
    ReadOnly,
}

/// Concrete tool executor scoped to one managed workspace.
pub struct WorkspaceDeveloperTools {
    pub(super) root: PathBuf,
    pub(super) access_policy: WorkspaceAccessPolicy,
    grounding: GroundingEvidence,
    ownership: WorkspaceOwnership,
    mode: WorkspaceToolMode,
    in_place_scope: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    command_evidence: CommandEvidence,
    command_budget: Option<CommandBudget>,
    receipts: Option<EffectReceiptLedger>,
    resources: CommandResources,
    command_runtime: Option<crate::CommandRuntime>,
    active_commands: ActiveCommandLedger,
    tools_without_delivery_progress: u16,
    progress_nudges: u8,
    progress_feedback_pending: bool,
    inspection_progress: inspection_progress::InspectionProgress,
    checkpoint_observer: Option<ToolCheckpointObserver>,
    prepared_mutations: Vec<PreparedMutation>,
    pub(super) protection_view: Option<std::sync::Arc<dyn crate::ConversationView>>,
}

impl WorkspaceDeveloperTools {
    pub(crate) fn with_in_place_scope(
        mut self,
        scope: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    ) -> Self {
        self.in_place_scope = scope;
        self
    }

    pub(crate) fn with_checkpoint_observer(mut self, observer: ToolCheckpointObserver) -> Self {
        self.checkpoint_observer = Some(observer);
        self
    }

    pub const fn grounding(&self) -> &GroundingEvidence {
        &self.grounding
    }

    pub(crate) const fn ownership(&self) -> &WorkspaceOwnership {
        &self.ownership
    }

    pub(crate) fn verification_evidence(&self) -> String {
        self.command_evidence.render()
    }

    pub(crate) fn successful_commands(&self) -> Vec<super::SuccessfulCommand> {
        self.command_evidence.successful()
    }

    fn permission_denial(&self, tool_name: &str) -> Option<String> {
        let permissions = self.protection_view.as_ref()?.effective_permissions();
        first_missing_permission(tool_name, permissions).map(|capability| {
            format!(
                "{tool_name} is disabled by the current {} permission; inspect /permissions",
                permission_name(capability)
            )
        })
    }
}

fn first_missing_permission(
    tool_name: &str,
    permissions: HostPermissions,
) -> Option<PermissionCapability> {
    required_permissions(tool_name)?
        .iter()
        .copied()
        .find(|capability| !permissions.allows(*capability))
}

fn required_permissions(tool_name: &str) -> Option<&'static [PermissionCapability]> {
    use PermissionCapability::{Network, Process, Read, Write};
    match tool_name {
        "workspace_list" | "workspace_search" | "workspace_read" => Some(&[Read]),
        "workspace_scope" | "workspace_write" | "workspace_patch" | "workspace_remove" => {
            Some(&[Read, Write])
        }
        "run_command" | "command_start" | "command_stdin" | "command_resize" | "command_signal" => {
            Some(&[Read, Write, Process, Network])
        }
        // Observation and cleanup of an already-owned process remain available after revocation.
        "command_poll" | "command_recover" | "command_cancel" => Some(&[]),
        _ => None,
    }
}

const fn permission_name(capability: PermissionCapability) -> &'static str {
    match capability {
        PermissionCapability::Read => "read",
        PermissionCapability::Write => "write",
        PermissionCapability::Process => "process",
        PermissionCapability::Network => "network",
    }
}

#[cfg(test)]
fn test_command_runtime(root: &std::path::Path) -> crate::CommandRuntime {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    crate::CommandRuntime::open_for_test(
        root,
        peritus_types::RunId::new(bytes).expect("test command run ID"),
    )
}

impl DeveloperToolExecutor for WorkspaceDeveloperTools {
    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if matches!(call.name().as_str(), "workspace_list" | "workspace_search" | "workspace_read")
        {
            DeveloperToolEffect::ReadOnly
        } else {
            DeveloperToolEffect::MutationCapable
        }
    }

    fn required_tool_name(&self) -> Option<&str> {
        self.grounding.required_tool_name()
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        // Live permissions are checked before access-policy refresh, in-place enrollment,
        // receipts, checkpoints, or any filesystem/process effect.
        if let Some(detail) = self.permission_denial(call.name().as_str()) {
            return observation(&object(vec![("error", Value::String(detail))]), true);
        }
        self.refresh_hard_constraints();
        if let Err(detail) = self.access_policy.authorize(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(detail))]), true);
        }
        if self.mode == WorkspaceToolMode::ReadOnly
            && !matches!(
                call.name().as_str(),
                "workspace_list" | "workspace_search" | "workspace_read"
            )
        {
            return observation(
                &object(vec![(
                    "error",
                    Value::String("this role has read-only workspace access".to_owned()),
                )]),
                true,
            );
        }
        // Reject malformed proposals before enrolling paths, creating effect receipts, or
        // allowing optional fields of the wrong type to silently become execution defaults.
        if let Err(error) = super::arguments::validate(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(error.to_string()))]), true);
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
            return Ok(observation);
        }
        if let Err(error) = self.prepare_in_place(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(error.to_string()))]), true);
        }
        if effect {
            if let Err(error) = self.prepare_effect_checkpoint(call.name().as_str(), &arguments) {
                return observation(
                    &object(vec![("error", Value::String(error.to_string()))]),
                    true,
                );
            }
            if let Some(observation) = self.begin_effect(call, &arguments)? {
                return Ok(observation);
            }
        }
        let result = self.dispatch_tool(call, &arguments);
        let (mut value, is_error, accepted) = match result {
            Ok(value) => {
                let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                (value, is_error, true)
            }
            Err(error) => {
                let value = object(vec![("error", Value::String(error.to_string()))]);
                (value, true, false)
            }
        };
        if accepted {
            self.active_commands.observe(
                &self.root,
                &mut value,
                &mut self.ownership,
                &mut self.command_evidence,
            )?;
        }
        if effect {
            self.receipts
                .as_mut()
                .ok_or_else(|| tool("writable tools have no effect receipt ledger"))?
                .complete(&value, is_error)?;
        }
        if accepted {
            self.record_checkpoint(call.name().as_str(), &arguments, &value)?;
        }
        if accepted {
            self.record_success(call.name().as_str(), &arguments, &value);
        }
        self.observe_delivery_progress(call.name().as_str(), &arguments, &value, accepted);
        if self.mode == WorkspaceToolMode::ReadWrite {
            self.inspection_progress.observe(call.name().as_str(), &arguments, &value);
        }
        observation(&value, is_error)
    }

    fn completion_blocker(&self) -> Option<String> {
        self.grounding.validate().err().map(str::to_owned)
    }

    fn take_progress_feedback(&mut self) -> Option<String> {
        if let Some(feedback) = self.inspection_progress.feedback() {
            return Some(feedback);
        }
        if !std::mem::take(&mut self.progress_feedback_pending) {
            return None;
        }
        Some(PROGRESS_FEEDBACK.to_owned())
    }

    fn continuation_blocker(&self) -> Option<String> {
        self.inspection_progress.blocker()
    }
}

mod effects;

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;
