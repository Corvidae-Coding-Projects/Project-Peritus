//! Typed observations shared by the staged candidate routes and controller session.

use serde::Serialize;

#[derive(Serialize)]
pub(in crate::native_controller) struct InjectedCandidate {
    pub(in crate::native_controller) checkpoint: String,
    pub(in crate::native_controller) claim_fence: Option<u64>,
    pub(in crate::native_controller) request_sha256: Option<String>,
    pub(in crate::native_controller) effect_path: Option<String>,
    pub(in crate::native_controller) effect_sha256: Option<String>,
    pub(in crate::native_controller) effect_bytes: Option<u64>,
    pub(in crate::native_controller) artifact_sha256: Option<String>,
    pub(in crate::native_controller) artifact_bytes: Option<u64>,
    pub(in crate::native_controller) snapshot: Option<SnapshotObservation>,
    pub(in crate::native_controller) lease: Option<LeaseObservation>,
    pub(in crate::native_controller) patch: Option<PatchObservation>,
    pub(in crate::native_controller) gate: Option<GateObservation>,
    pub(in crate::native_controller) promotion: Option<PromotionCheckpoint>,
    pub(in crate::native_controller) killed_exit: String,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct RecoveredCandidate {
    pub(in crate::native_controller) observation: String,
    pub(in crate::native_controller) destination_reconciled: bool,
    pub(in crate::native_controller) external_effects: u64,
    pub(in crate::native_controller) duplicate_effects: u64,
    pub(in crate::native_controller) exact_fence_acknowledged: bool,
    pub(in crate::native_controller) pending_claims: u64,
    pub(in crate::native_controller) committed_events: Option<u64>,
    pub(in crate::native_controller) aggregate_heads: Option<u64>,
    pub(in crate::native_controller) journal_sha256: String,
    pub(in crate::native_controller) journal_bytes: u64,
    pub(in crate::native_controller) effect_sha256: Option<String>,
    pub(in crate::native_controller) effect_bytes: Option<u64>,
    pub(in crate::native_controller) artifact_sha256: Option<String>,
    pub(in crate::native_controller) artifact_bytes: Option<u64>,
    pub(in crate::native_controller) snapshot: Option<SnapshotObservation>,
    pub(in crate::native_controller) lease: Option<LeaseObservation>,
    pub(in crate::native_controller) patch: Option<PatchObservation>,
    pub(in crate::native_controller) gate: Option<GateObservation>,
    pub(in crate::native_controller) promotion: Option<PromotionObservation>,
    pub(in crate::native_controller) elapsed_millis: u64,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct SnapshotObservation {
    pub(in crate::native_controller) commit: Option<String>,
    pub(in crate::native_controller) tree: String,
    pub(in crate::native_controller) reference: String,
    pub(in crate::native_controller) manifest_sha256: Option<String>,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct LeaseObservation {
    pub(in crate::native_controller) request_sha256: String,
    pub(in crate::native_controller) state_revision: Option<u64>,
    pub(in crate::native_controller) state_sha256: Option<String>,
    pub(in crate::native_controller) producing_position: Option<u64>,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct PatchObservation {
    pub(in crate::native_controller) identity: String,
    pub(in crate::native_controller) postimage: Option<String>,
    pub(in crate::native_controller) receipt_manifest: Option<String>,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct GateObservation {
    pub(in crate::native_controller) request_sha256: String,
    pub(in crate::native_controller) plan_sha256: String,
    pub(in crate::native_controller) successor_sha256: Option<String>,
    pub(in crate::native_controller) checkpoint_sha256: Option<String>,
    pub(in crate::native_controller) state_revision: Option<u64>,
    pub(in crate::native_controller) producing_position: Option<u64>,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct PromotionCheckpoint {
    pub(in crate::native_controller) proposal_sha256: String,
    pub(in crate::native_controller) authorization_sha256: String,
    pub(in crate::native_controller) campaign_before_sha256: String,
    pub(in crate::native_controller) pointer_before_sha256: String,
    pub(in crate::native_controller) campaign_after_sha256: String,
    pub(in crate::native_controller) pointer_after_sha256: String,
    pub(in crate::native_controller) approval_revision: Option<u64>,
    pub(in crate::native_controller) first_position: Option<u64>,
    pub(in crate::native_controller) last_position: Option<u64>,
    pub(in crate::native_controller) committed: bool,
}

#[derive(Serialize)]
pub(in crate::native_controller) struct PromotionObservation {
    pub(in crate::native_controller) proposal_sha256: String,
    pub(in crate::native_controller) authorization_sha256: Option<String>,
    pub(in crate::native_controller) campaign_sha256: String,
    pub(in crate::native_controller) pointer_sha256: String,
    pub(in crate::native_controller) approval_revision: u64,
    pub(in crate::native_controller) approval_position: u64,
    pub(in crate::native_controller) committed_events: u64,
    pub(in crate::native_controller) aggregate_heads: u64,
    pub(in crate::native_controller) committed: bool,
}
