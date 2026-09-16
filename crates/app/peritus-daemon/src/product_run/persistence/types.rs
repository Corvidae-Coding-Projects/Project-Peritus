//! Existing persisted JSON fields, separated from I/O and recovery logic.

use super::{PersistedCheckpoint, interaction};
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedRecord {
    #[serde(default)]
    pub(super) interaction: Option<interaction::PersistedInteraction>,
    pub(super) run_id: String,
    pub(super) workspace_id: String,
    pub(super) writer: String,
    pub(super) reviewer: String,
    pub(super) fixer: String,
    pub(super) phase: u16,
    pub(super) cycle: u32,
    pub(super) task: String,
    pub(super) status: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) summary: String,
    #[serde(default)]
    pub(super) user_cancelled: bool,
    #[serde(default)]
    pub(super) finding_state: String,
    #[serde(default)]
    pub(super) deliverable: Option<PersistedDeliverable>,
    #[serde(default)]
    pub(super) messages: Vec<PersistedMessage>,
    #[serde(default)]
    pub(super) progress: PersistedProgress,
    #[serde(default)]
    pub(super) checkpoint: Option<PersistedCheckpoint>,
    #[serde(default)]
    pub(super) settlement_cause: Option<u16>,
    #[serde(default)]
    pub(super) resume_state: Option<Vec<u8>>,
    #[serde(default)]
    pub(super) remaining_work: Vec<String>,
    #[serde(default)]
    pub(super) interruption_cause: String,
    #[serde(default)]
    pub(super) candidate_actionable: Option<bool>,
    #[serde(default)]
    pub(super) preview_page: Option<Vec<u8>>,
    #[serde(default)]
    pub(super) preview_operations: Vec<PersistedPreviewOperation>,
    #[serde(default)]
    pub(super) preview_outputs: Vec<PersistedPreviewOutput>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedPreviewOperation {
    pub(super) operation: [u8; 16],
    pub(super) fingerprint: [u8; 32],
    pub(super) accepted_revision: u64,
    #[serde(default)]
    pub(super) result_sequence: u64,
    #[serde(default)]
    pub(super) completed_sequence: u64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedPreviewOutput {
    pub(super) launch: [u8; 16],
    pub(super) stdout: String,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct PersistedProgress {
    pub(super) started_unix_millis: u64,
    pub(super) last_effect_unix_millis: u64,
    pub(super) model_requests: u32,
    pub(super) tool_calls: u32,
    pub(super) retries: u32,
    #[serde(default)]
    pub(super) provider_failovers: u32,
    pub(super) compactions: u32,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) provider_cost_microunits: u64,
    pub(super) usage_observations: u32,
    #[serde(default)]
    pub(super) workspace_bytes: u64,
    #[serde(default)]
    pub(super) workspace_growth_bytes: u64,
    #[serde(default)]
    pub(super) peak_rss_bytes: u64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedMessage {
    pub(super) role: u16,
    pub(super) content: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedDeliverable {
    pub(super) workspace_path: String,
    pub(super) changed_paths: Vec<String>,
    pub(super) successful_commands: Vec<String>,
    pub(super) run_instructions: String,
    #[serde(default)]
    pub(super) qualification: Option<u16>,
    pub(super) accepted: bool,
    pub(super) commit_revision: String,
    pub(super) export_path: String,
    pub(super) discarded: bool,
}
