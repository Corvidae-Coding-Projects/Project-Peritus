//! Concrete bounded filesystem and structured-command developer tools.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use peritus_agent::{
    DeveloperLoopError, DeveloperToolEffect, DeveloperToolExecutor, DeveloperToolObservation,
};
use peritus_model_protocol::{CompletedToolCall, Message};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{
    access_policy::WorkspaceAccessPolicy,
    command_budget::CommandBudget,
    effect::atomic_write,
    evidence::{CommandEvidence, ReviewerEvidenceSources},
    grounding::GroundingEvidence,
    inspection,
    inspection_cancellation::InspectionCancellation,
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

const PROGRESS_FEEDBACK: &str = "The harness observed a long inspection sequence without a workspace mutation or successful declared external effect. Choose the shortest concrete delivery step now. If a standard capability is missing and the active disposable task authorizes installation, use the available package or runtime manager before hand-writing a substitute. Otherwise write or apply the requested result, then verify it. Continue inspecting only when a specific unresolved requirement still needs evidence.";

mod active;
mod checkpoint_observer;
mod command;
mod construction;
mod in_place;
mod inspection_progress;
mod read_reuse;
mod wait;

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
    design_grounding_only: bool,
    ownership: WorkspaceOwnership,
    mode: WorkspaceToolMode,
    in_place_scope: Option<crate::workspace_delivery::scope::ScopedBaseline>,
    command_evidence: CommandEvidence,
    reviewer_evidence: Option<ReviewerEvidenceSources>,
    command_budget: Option<CommandBudget>,
    receipts: Option<EffectReceiptLedger>,
    resources: CommandResources,
    pub(super) inspection_cancellation: InspectionCancellation,
    pub(super) command_runtime: Option<crate::CommandRuntime>,
    active_commands: ActiveCommandLedger,
    tools_without_delivery_progress: u16,
    progress_nudges: u8,
    progress_feedback_pending: bool,
    inspection_progress: inspection_progress::InspectionProgress,
    pub(super) rust_reliability: crate::engineering_workflow::rust_reliability::Activation,
    checkpoint_observer: Option<ToolCheckpointObserver>,
    prepared_mutations: Vec<PreparedMutation>,
    pub(super) protection_view: Option<std::sync::Arc<dyn crate::ConversationView>>,
}

impl WorkspaceDeveloperTools {
    /// The designer hands off after the run's one grounding check. Source-specific questions
    /// belong to implementation; independent review keeps its ordinary read-only tools.
    pub(crate) const fn for_design(mut self) -> Self {
        self.design_grounding_only = true;
        self
    }

    /// Attaches reusable source observations for one host-owned run/workspace scope.
    /// The trace must be outside the model-writable workspace. Observations grant no
    /// permissions; current access checks and full content-hash validation still apply.
    #[must_use]
    pub fn with_observation_receipts(
        mut self,
        trace: &Path,
        run: peritus_types::RunId,
        workspace: peritus_types::WorkspaceId,
    ) -> Self {
        if self.permission_denial("workspace_read").is_some() {
            return self;
        }
        let mut scope = [0; 32];
        scope[..16].copy_from_slice(run.as_bytes());
        scope[16..].copy_from_slice(workspace.as_bytes());
        let policy = &self.access_policy;
        self.grounding.restore(trace.with_extension("observations.json"), scope, |relative| {
            policy.authorize("workspace_read", &serde_json::json!({"path": relative})).is_ok()
        });
        crate::efficiency::event(
            trace,
            serde_json::json!({"event":"grounding-reuse", "files_revalidated":self.grounding.reused_count(),"file_hash_validation_bytes":self.grounding.validation_bytes(),"semantics":"hash checks perform local IO; reuse reduces model-visible rediscovery, not all file reads"}),
        );
        self
    }
    pub(crate) fn with_computer_runtime(mut self, runtime: crate::CommandRuntime) -> Self {
        if self.command_runtime.is_none() {
            self.command_runtime = Some(runtime);
        }
        self
    }
    pub(crate) fn with_reviewer_evidence_sources(
        mut self,
        evidence: ReviewerEvidenceSources,
    ) -> Self {
        self.reviewer_evidence = Some(evidence);
        self
    }

    pub(crate) fn with_inspection_cancellation(
        mut self,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        provider: peritus_provider_core::CancellationToken,
    ) -> Self {
        self.inspection_cancellation = InspectionCancellation::new(cancelled, provider);
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

    fn read_reviewer_evidence(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let archive = self
            .reviewer_evidence
            .as_ref()
            .ok_or_else(|| tool("full historical evidence is unavailable in this role turn"))?;
        let section = match arguments.get("section") {
            None => "developer_commands",
            Some(Value::String(section)) => section.as_str(),
            Some(_) => return Err(tool("section must be a named string")),
        };
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .ok_or_else(|| tool("offset must be a nonnegative integer"))?;
        let maximum = match arguments.get("max_bytes") {
            None => super::DEFAULT_INSPECTION_PAGE_BYTES,
            Some(value) => value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| (1..=inspection::MAX_PAGE_BYTES).contains(value))
                .ok_or_else(|| tool("max_bytes must be between 1 and 524288"))?,
        };
        archive.read(section, offset, maximum)
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

        self.rust_reliability.observe(
            call.name().as_str(),
            arguments,
            value,
            accepted && !is_error,
            mutation_boundary,
        );
        observation(value, is_error)
    }
}

pub(super) fn reviewer_evidence_page(
    section: &str,
    evidence: &str,
    offset: u64,
    start: usize,
    maximum: usize,
) -> Result<Value, DeveloperLoopError> {
    reviewer_evidence_page_with_metadata(section, evidence, offset, start, maximum, &[])
}

pub(super) fn reviewer_evidence_page_with_metadata(
    section: &str,
    evidence: &str,
    offset: u64,
    start: usize,
    maximum: usize,
    metadata: &[(&str, Value)],
) -> Result<Value, DeveloperLoopError> {
    let digest = Sha256::digest(evidence.as_bytes());
    let mut sha256 = String::with_capacity(64);
    for byte in digest {
        write!(sha256, "{byte:02x}").expect("writing to String cannot fail");
    }
    let total_bytes =
        u64::try_from(evidence.len()).map_err(|_| tool("evidence length overflow"))?;
    let mut raw_end = start.saturating_add(maximum).min(evidence.len());
    while raw_end > start && !evidence.is_char_boundary(raw_end) {
        raw_end -= 1;
    }
    let mut boundaries = vec![start];
    boundaries.extend(
        evidence[start..raw_end].char_indices().skip(1).map(|(relative, _)| start + relative),
    );
    if raw_end > start {
        boundaries.push(raw_end);
    }

    let make_page = |end: usize| -> Result<Value, DeveloperLoopError> {
        let next_offset = if end < evidence.len() {
            Value::from(u64::try_from(end).map_err(|_| tool("next offset overflow"))?)
        } else {
            Value::Null
        };
        let mut value = object(vec![
            ("bytes", Value::String(evidence[start..end].to_owned())),
            ("next_offset", next_offset),
            ("offset", Value::from(offset)),
            ("section", Value::String(section.to_owned())),
            ("sha256", Value::String(sha256.clone())),
            ("total_bytes", Value::from(total_bytes)),
        ]);
        for (key, extra) in metadata {
            value[*key] = extra.clone();
        }
        Ok(value)
    };
    // EOF replaces the numeric continuation with null, which can shrink the envelope.
    // Check that endpoint before searching the monotonically growing nonterminal pages.
    if raw_end == evidence.len() {
        let complete_page = make_page(raw_end)?;
        if inspection::encoded_len(&complete_page)? <= maximum {
            return Ok(complete_page);
        }
    }
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(1);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        let candidate = make_page(boundaries[middle])?;
        if inspection::encoded_len(&candidate)? <= maximum {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let end = boundaries[low];
    if end == start
        && (start < evidence.len() || inspection::encoded_len(&make_page(end)?)? > maximum)
    {
        let next_boundary = evidence[start..]
            .char_indices()
            .nth(1)
            .map_or(evidence.len(), |(relative, _)| start + relative);
        let minimum_end = next_boundary.max(start);
        let required = inspection::encoded_len(&make_page(minimum_end)?)?;
        return Err(tool(format!(
            "max_bytes is too small to return a progressing page; request at least {required} bytes"
        )));
    }
    make_page(end)
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
        "workspace_list"
        | "workspace_search"
        | "workspace_read"
        | "attachment_read"
        | "developer_evidence_read"
        | "skill_read"
        | "skill_list"
        | "view_image"
        | "computer_snapshot"
        | "command_output"
        | "command_wait"
        | "web_read"
        | "web_find" => Some(&[Read]),
        "web_search" | "web_open" => Some(&[Read, Network]),
        "computer_status" => Some(&[Read, Process]),
        "browser_control" => Some(&[Read, Write, Process, Network]),
        "desktop_control" => Some(&[Read, Write, Process]),
        "computer_close" | "command_poll" | "command_recover" | "command_cancel" => Some(&[]),
        "workspace_scope" | "workspace_write" | "workspace_patch" | "workspace_remove" => {
            Some(&[Read, Write])
        }
        "run_command" | "command_start" | "command_stdin" | "command_resize" | "command_signal" => {
            Some(&[Read, Write, Process])
        }
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

impl DeveloperToolExecutor for WorkspaceDeveloperTools {
    fn system_guidance(&mut self) -> Option<String> {
        // Role prompts also contain design, findings and archived evidence. Language hints
        // come only from the host's user-authored authority context, never those sources.
        if let Some(view) = &self.protection_view {
            self.rust_reliability.task(&view.reference_authority_context());
        }
        Some(format!(
            "{}\n{}\n{}\n{}",
            crate::bundled_skills::instructions("web-research"),
            self.rust_reliability.guidance().unwrap_or_default(),
            self.grounding.guidance(),
            if self.design_grounding_only {
                "DESIGN HANDOFF: Perform only the single grounding check if pending. Once complete, tools are closed for this design phase. Return the task plan from the request and available observations now. Do not invent source contracts; put specific unresolved source questions in the first implementation slice so the writer can read its actual target and get to work. No repeat discovery after retries, steering, or role changes."
            } else {
                ""
            },
        ))
    }

    fn effect(&self, call: &CompletedToolCall) -> DeveloperToolEffect {
        if matches!(
            call.name().as_str(),
            "workspace_list"
                | "workspace_search"
                | "workspace_read"
                | "attachment_read"
                | "developer_evidence_read"
                | "skill_read"
                | "skill_list"
                | "view_image"
                | "computer_snapshot"
                | "computer_status"
                | "web_search"
                | "web_open"
                | "web_read"
                | "web_find"
        ) {
            DeveloperToolEffect::ReadOnly
        } else {
            DeveloperToolEffect::MutationCapable
        }
    }

    fn required_tool_name(&self) -> Option<&str> {
        self.grounding.required_tool_name()
    }

    fn tool_calls_allowed(&self) -> bool {
        !self.design_grounding_only || !self.grounding.is_complete()
    }

    fn execute(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let output = self.execute_with_read(call, None)?;
        let args =
            serde_json::from_slice(call.arguments().canonical_bytes()).unwrap_or(Value::Null);
        let value = serde_json::from_slice(output.output.canonical_bytes()).unwrap_or(Value::Null);
        self.inspection_progress.observe(call.name().as_str(), &args, &value, false);
        Ok(output)
    }

    fn reuse_workspace_read(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Option<DeveloperToolObservation>, DeveloperLoopError> {
        if call.name().as_str() != "workspace_read" {
            return Ok(None);
        }
        let output = self.execute_with_read(call, Some(observation))?;
        let args =
            serde_json::from_slice(call.arguments().canonical_bytes()).unwrap_or(Value::Null);
        let value = serde_json::from_slice(output.output.canonical_bytes()).unwrap_or(Value::Null);
        self.inspection_progress.observe(call.name().as_str(), &args, &value, false);
        Ok(Some(output))
    }

    fn observe_model_context(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.inspection_progress.observe_model_context(messages);
        Ok(())
    }

    fn observation_images(
        &mut self,
        call: &CompletedToolCall,
        observation: &DeveloperToolObservation,
    ) -> Result<Vec<peritus_model_protocol::MediaInput>, DeveloperLoopError> {
        self.computer_observation_images(call, observation)
    }

    fn completion_blocker(&self) -> Option<String> {
        self.grounding.validate().err().map(str::to_owned)
    }

    fn take_progress_feedback(&mut self) -> Option<String> {
        if let Some(feedback) = self.inspection_progress.feedback() {
            return Some(
                self.rust_reliability
                    .refresh()
                    .map_or_else(|| feedback.clone(), |refresh| format!("{feedback}\n{refresh}")),
            );
        }
        if !std::mem::take(&mut self.progress_feedback_pending) {
            return None;
        }
        Some(PROGRESS_FEEDBACK.to_owned())
    }
}

mod effects;

impl WorkspaceDeveloperTools {
    fn execute_with_read(
        &mut self,
        call: &CompletedToolCall,
        reused: Option<&DeveloperToolObservation>,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let arguments: Value = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| tool(error.to_string()))?;
        // Live permissions are checked before access-policy refresh, in-place enrollment,
        // receipts, checkpoints, or any filesystem/process effect.
        if let Some(detail) = self.permission_denial(call.name().as_str()) {
            return observation(&object(vec![("error", Value::String(detail))]), true);
        }
        if !self.tool_calls_allowed() {
            return observation(
                &object(vec![("error", Value::String(
                    "The single grounding check is complete. Return the design now; the implementer resolves specific target questions while working.".to_owned(),
                ))]),
                true,
            );
        }
        self.refresh_hard_constraints();
        if let Err(detail) = self.access_policy.authorize(call.name().as_str(), &arguments) {
            return observation(&object(vec![("error", Value::String(detail))]), true);
        }
        if self.mode == WorkspaceToolMode::ReadOnly
            && !matches!(
                call.name().as_str(),
                "workspace_list"
                    | "workspace_search"
                    | "workspace_read"
                    | "attachment_read"
                    | "developer_evidence_read"
                    | "skill_read"
                    | "skill_list"
                    | "view_image"
                    | "computer_snapshot"
                    | "computer_status"
                    | "web_search"
                    | "web_open"
                    | "web_read"
                    | "web_find"
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
        self.execute_authorized(call, &arguments, reused)
    }

    fn execute_authorized(
        &mut self,
        call: &CompletedToolCall,
        arguments: &Value,
        reused: Option<&DeveloperToolObservation>,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
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
                | "browser_control"
                | "desktop_control"
                | "computer_close"
        ) && self.mode == WorkspaceToolMode::ReadWrite;
        if effect && let Some(observation) = self.replay_effect(call, arguments)? {
            return Ok(observation);
        }
        if let Err(error) = self.prepare_in_place(call.name().as_str(), arguments) {
            return observation(&object(vec![("error", Value::String(error.to_string()))]), true);
        }
        if effect {
            if let Err(error) = self.prepare_effect_checkpoint(call.name().as_str(), arguments) {
                return observation(
                    &object(vec![("error", Value::String(error.to_string()))]),
                    true,
                );
            }
            if let Some(observation) = self.begin_effect(call, arguments)? {
                return Ok(observation);
            }
        }
        let result = self.dispatch_reusing_read(call, arguments, reused);
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
