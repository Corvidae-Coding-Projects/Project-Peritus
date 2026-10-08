//! Concrete bounded filesystem and structured-command developer tools.

use std::{fs, path::PathBuf};

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{CompletedToolCall, Message};
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
    reference::ExplicitReferences,
    removal,
    resources::CommandResources,
    wire::{object, observation, required_string, string},
};
use crate::control::{HostPermissions, PermissionCapability};
const TOOLS_WITHOUT_DELIVERY_PROGRESS: u16 = 12;
const MAX_PROGRESS_NUDGES: u8 = 2;
const PROGRESS_FEEDBACK: &str = "The harness observed a long inspection sequence without a workspace mutation or successful declared external effect. Choose the shortest concrete delivery step now. If a standard capability is missing and the active disposable task authorizes installation, use the available package or runtime manager before hand-writing a substitute. Otherwise write or apply the requested result, then verify it. Continue inspecting only when a specific unresolved requirement still needs evidence.";

mod active;
mod checkpoint_observer;
mod command;
mod construction;
mod dispatch;
mod in_place;
mod inspection_progress;
mod literal_patch;
pub(in crate::developer_tools) mod sources;

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
    pub(super) references: ExplicitReferences,
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
    directory_listings: Option<inspection::DirectoryListingOwner>,
    request_sources: sources::RequestSourceProgress,
    checkpoint_observer: Option<ToolCheckpointObserver>,
    checkpoint_view: Option<std::sync::Arc<dyn crate::ConversationView>>,
    prepared_mutations: Vec<PreparedMutation>,
    checkpoint_targets: Vec<(String, crate::WorkspaceMutationKind)>,
    pub(super) protection_view: Option<std::sync::Arc<dyn crate::ConversationView>>,
}

impl WorkspaceDeveloperTools {
    pub(crate) fn with_directory_listing_owner(
        mut self,
        owner: inspection::DirectoryListingOwner,
    ) -> Self {
        self.directory_listings = Some(owner);
        self
    }

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

    pub(crate) fn with_checkpoint_view(
        mut self,
        view: std::sync::Arc<dyn crate::ConversationView>,
    ) -> Self {
        self.checkpoint_view = Some(view);
        self
    }

    pub const fn grounding(&self) -> &GroundingEvidence {
        &self.grounding
    }

    pub(crate) fn with_grounding(mut self, mut grounding: GroundingEvidence) -> Self {
        grounding.bind_workspace(&self.root);
        self.grounding = grounding;
        self.restore_request_source_evidence();
        self
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

    fn request_source_denial(&self, tool_name: &str) -> Option<String> {
        if matches!(
            tool_name,
            "request_sources" | "request_source_read"
                | "command_poll" | "command_recover" | "command_cancel"
        ) {
            return None;
        }
        let view = self.protection_view.as_ref()?;
        match view.request_sources_required() {
            Ok(false) => None,
            Ok(true) if self.request_sources.is_complete_at(view.request_source_binding()) => None,
            Ok(true) => Some(
                "read every user_request body marked requiresRead through request_sources and request_source_read before using workspace tools"
                    .to_owned(),
            ),
            Err(error) => Some(format!(
                "authoritative request sources are unavailable: {error}; workspace tools remain disabled"
            )),
        }
    }

    fn finish_observation(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        value: &Value,
        is_error: bool,
        accepted: bool,
        mutation_boundary: bool,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        if accepted {
            self.record_success(call.name().as_str(), arguments, value);
        }
        self.observe_delivery_progress(call.name().as_str(), arguments, value, accepted);
        self.inspection_progress.observe(call.name().as_str(), arguments, value, mutation_boundary);
        observation(value, is_error)
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
    use PermissionCapability::{Process, Read, Write};
    match tool_name {
        // Governing user messages and retained assistant history are part of the conversation.
        // Reading them grants no workspace capability and remains possible before permissions.
        "request_sources" | "request_source_read" => None,
        "context_sources" | "context_source_read" | "workspace_list" | "workspace_search"
        | "workspace_read" => Some(&[Read]),
        "workspace_scope" | "workspace_write" | "workspace_patch" | "workspace_remove" => {
            Some(&[Read, Write])
        }
        "run_command" | "command_start" | "command_stdin" | "command_resize" | "command_signal" => {
            Some(&[Read, Write, Process])
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
        if matches!(
            call.name().as_str(),
            "request_sources" | "request_source_read" | "context_sources" | "context_source_read"
                | "workspace_list" | "workspace_search" | "workspace_read"
        ) {
            DeveloperToolEffect::ReadOnly
        } else {
            DeveloperToolEffect::MutationCapable
        }
    }

    fn required_tool_name(&self) -> Option<&str> {
        if let Some(view) = &self.protection_view {
            match view.request_sources_required() {
                Ok(true) => {
                    if let Some(tool) = self.request_sources.required_tool_at(view.request_source_binding()) {
                        return Some(tool);
                    }
                }
                Err(_) => return Some("request_sources"),
                Ok(false) => {}
            }
        }
        self.grounding.required_tool_name()
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        match self.prepare_dispatch(call)? {
            dispatch::PreparedTool::Observed(observation) => Ok(observation),
            dispatch::PreparedTool::Ready { arguments, effect } => {
                if effect
                    && let Err(error) =
                        self.prepare_effect_checkpoint(call.name().as_str(), &arguments)
                {
                    return observation(
                        &object(vec![("error", Value::String(error.to_string()))]),
                        true,
                    );
                }
                self.dispatch_prepared(call, &arguments, effect)
            }
        }
    }

    fn execute_async<'a>(
        &'a mut self,
        call: &'a CompletedToolCall,
    ) -> peritus_agent::DeveloperToolExecution<'a> {
        Box::pin(async move {
            match self.prepare_dispatch(call)? {
                dispatch::PreparedTool::Observed(observation) => Ok(observation),
                dispatch::PreparedTool::Ready { arguments, effect } => {
                    if effect
                        && let Err(error) = self
                            .prepare_effect_checkpoint_async(call.name().as_str(), &arguments)
                            .await
                    {
                        return observation(
                            &object(vec![("error", Value::String(error.to_string()))]),
                            true,
                        );
                    }
                    // Permission and protection changes received while waiting remain authoritative.
                    if let Some(detail) = self.permission_denial(call.name().as_str()) {
                        return observation(&object(vec![("error", Value::String(detail))]), true);
                    }
                    self.refresh_hard_constraints();
                    if let Err(detail) =
                        self.access_policy.authorize(call.name().as_str(), &arguments)
                    {
                        return observation(&object(vec![("error", Value::String(detail))]), true);
                    }
                    self.dispatch_prepared_async(call, &arguments, effect).await
                }
            }
        })
    }

    fn observe_model_context(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.grounding.recover_model_context(messages);
        self.restore_request_source_evidence();
        self.inspection_progress.observe_model_context(messages);
        Ok(())
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
}

mod effects;

#[cfg(test)]
mod receipt_tests;
#[cfg(test)]
mod tests;
