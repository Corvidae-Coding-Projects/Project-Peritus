//! Existing persisted JSON fields, separated from I/O and recovery logic.

use super::{PersistedCheckpoint, PersistedObligation, PersistedResumeRoot, interaction};
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedRecord {
    pub(super) format_version: u16,
    #[serde(default)]
    pub(super) record_revision: u64,
    #[serde(default)]
    pub(super) record_lineage_root: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) settlement_obligation: Option<PersistedObligation>,
    #[serde(default = "default_attempt_sequence")]
    pub(super) attempt_sequence: u64,
    #[serde(default)]
    pub(super) handoff_sequence: u64,
    #[serde(default)]
    pub(super) review_artifact_migration_version: u16,
    #[serde(default)]
    pub(super) handoff_recovery_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) rejected_finding_update: Option<PersistedRejectedFindingUpdate>,
    /// Last exact retry operation whose run-owned launch preparation was durably recorded.
    /// The JSON key remains `goal_resume` for backward-compatible format-version six records.
    #[serde(rename = "goal_resume", skip_serializing_if = "Option::is_none")]
    pub(super) attempt_admission: Option<[u8; 16]>,
    /// Every continuation operation whose run-owned launch preparation was persisted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) continuation_admissions: Vec<[u8; 16]>,
    /// Immutable control sources that make those admissions recoverable without reading current
    /// queue state. This is separate from the receipt-only compatibility field above.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) continuation_sources: Vec<PersistedContinuationSource>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) resume_state: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) resume_root: Option<PersistedResumeRoot>,
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

const fn default_attempt_sequence() -> u64 {
    1
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedRejectedFindingUpdate {
    pub(super) accepted_head: [u8; 32],
    pub(super) rejected_head: [u8; 32],
    pub(super) rejected_bytes: u64,
    pub(super) rejected_finding_state: String,
    pub(super) phase: u16,
    pub(super) cycle: u32,
    pub(super) status: String,
    pub(super) diff: String,
    pub(super) gates: String,
    pub(super) review: String,
    pub(super) summary: String,
    pub(super) error: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedContinuationSource {
    pub(super) operation: [u8; 16],
    pub(super) revision: u64,
    pub(super) generation: u64,
    pub(super) settled: bool,
}

#[derive(Serialize, Deserialize)]
pub(super) struct PersistedPreviewOperation {
    pub(super) operation: [u8; 16],
    pub(super) fingerprint: [u8; 32],
    pub(super) accepted_revision: u64,
    pub(super) result_sequence: u64,
    pub(super) completed_sequence: u64,
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
    #[serde(default)]
    pub(super) catalog_sequence: u64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) resource_telemetry: Option<PersistedResourceTelemetry>,
    #[serde(default)]
    pub(super) last_event: String,
    #[serde(default)]
    pub(super) provider_started_unix_millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) provider_request: Option<PersistedProviderRequest>,
    #[serde(
        default,
        rename = "provider_deadline_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) _legacy_provider_deadline_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedProviderRequest {
    pub(super) role: u8,
    pub(super) turn: u16,
    pub(super) attempt: u64,
    pub(super) request_id_digest: [u8; 32],
    pub(super) request_fingerprint: [u8; 32],
    pub(super) provider_profile_id: [u8; 16],
    pub(super) provider_profile_revision: u64,
    pub(super) provider_name_digest: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) native_session_digest: Option<[u8; 32]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) provider_selection_digest: Option<[u8; 32]>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedResourceTelemetry {
    pub(super) workspace: PersistedResourceMeasurement,
    pub(super) workspace_growth: PersistedResourceMeasurement,
    pub(super) peak_rss: PersistedResourceMeasurement,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub(super) enum PersistedResourceMeasurement {
    Measured {
        value: u64,
        coverage: PersistedResourceCoverage,
    },
    Partial {
        value: Option<u64>,
        coverage: PersistedResourceCoverage,
        cause: PersistedResourceCause,
    },
    Unavailable {
        coverage: PersistedResourceCoverage,
        cause: PersistedResourceCause,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedResourceCoverage {
    pub(super) entries_observed: u64,
    pub(super) directories_completed: u64,
    pub(super) items_unavailable: u64,
    pub(super) directories_pending: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub(super) enum PersistedResourceCause {
    NotObserved,
    LegacyUnspecified,
    BaselineEstablishedAfterStart,
    NotAuthorized,
    ScanInProgress,
    Cancelled,
    ArithmeticOverflow,
    WorkerUnavailable,
    InvalidPlatformData,
    PlatformCommandFailed {
        exit_code: Option<i32>,
        signal: Option<i32>,
    },
    Io {
        operation: PersistedResourceOperation,
        kind: PersistedResourceIoKind,
        raw_os_error: Option<i32>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PersistedResourceOperation {
    OpenDirectory,
    ReadDirectoryEntry,
    ReadMetadata,
    ReadProcessMemory,
    StartResourceObserver,
    StartProcessMemoryCommand,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PersistedResourceIoKind {
    NotFound,
    PermissionDenied,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    NotConnected,
    AddressInUse,
    AddressNotAvailable,
    BrokenPipe,
    AlreadyExists,
    WouldBlock,
    InvalidInput,
    InvalidData,
    TimedOut,
    WriteZero,
    Interrupted,
    Unsupported,
    UnexpectedEof,
    OutOfMemory,
    Other,
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
