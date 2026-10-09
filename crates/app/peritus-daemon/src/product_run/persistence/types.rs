//! Existing persisted JSON fields, separated from I/O and recovery logic.

use super::{PersistedCheckpoint, interaction};
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedRecord {
    pub(super) format_version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) goal_resume: Option<[u8; 16]>,
    pub(super) interaction: interaction::PersistedInteraction,
    pub(super) run_id: String,
    pub(super) workspace_id: String,
    pub(super) writer: String,
    pub(super) reviewer: String,
    pub(super) fixer: String,
    pub(super) phase: u16,
    pub(super) cycle: u32,
    pub(super) execution_task: String,
    pub(super) task: String,
    pub(super) status: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) summary: String,
    pub(super) user_cancelled: bool,
    pub(super) finding_state: String,
    pub(super) deliverable: Option<PersistedDeliverable>,
    pub(super) progress: PersistedProgress,
    pub(super) checkpoint: Option<PersistedCheckpoint>,
    pub(super) settlement_cause: Option<u16>,
    pub(super) resume_state: Option<Vec<u8>>,
    pub(super) remaining_work: Vec<String>,
    pub(super) interruption_cause: String,
    pub(super) candidate_actionable: bool,
    pub(super) task_baseline_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) task_baseline: Option<String>,
    pub(super) preview_page: Option<Vec<u8>>,
    pub(super) preview_operations: Vec<PersistedPreviewOperation>,
    pub(super) preview_outputs: Vec<PersistedPreviewOutput>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedPreviewOperation {
    pub(super) operation: [u8; 16],
    pub(super) fingerprint: [u8; 32],
    pub(super) accepted_revision: u64,
    pub(super) result_sequence: u64,
    pub(super) completed_sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) behavior_evidence: Option<super::super::preview_evidence::PreviewBehaviorEvidence>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedPreviewOutput {
    pub(super) launch: [u8; 16],
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) truncated: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct PersistedProgress {
    pub(super) started_unix_millis: u64,
    pub(super) last_effect_unix_millis: u64,
    pub(super) model_requests: u32,
    pub(super) tool_calls: u32,
    pub(super) retries: u32,
    pub(super) provider_failovers: u32,
    pub(super) compactions: u32,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) provider_cost_microunits: u64,
    pub(super) usage_observations: u32,
    pub(super) workspace_bytes: u64,
    pub(super) workspace_growth_bytes: u64,
    pub(super) peak_rss_bytes: u64,
    #[serde(default)]
    pub(super) last_event: String,
    #[serde(default)]
    pub(super) provider_started_unix_millis: Option<u64>,
    #[serde(
        default,
        rename = "provider_deadline_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) _legacy_provider_deadline_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedDeliverable {
    pub(super) workspace_path: String,
    pub(super) changed_paths: Vec<String>,
    pub(super) successful_commands: Vec<String>,
    pub(super) run_instructions: String,
    pub(super) qualification: u16,
    pub(super) accepted: bool,
    pub(super) commit_revision: String,
    pub(super) export_path: String,
    pub(super) discarded: bool,
}
