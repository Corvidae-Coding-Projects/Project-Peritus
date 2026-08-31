//! Canonical one-line H1 controller responses.

use std::io::Write as _;

use serde::Serialize;

use super::evidence::EvidenceDocument;
use super::request::{BoundRequest, ScenarioRoute, Stage};

#[derive(Serialize)]
struct ResponseDocument<'a, T> {
    schema_version: u8,
    stage: &'static str,
    sequence: u8,
    instance_id: &'a str,
    scenario_id: &'a str,
    request_sha256: &'a str,
    payload: T,
}

#[derive(Serialize)]
pub(super) struct PreparePayload {
    pub(super) terminal: &'static str,
    pub(super) journal_head_sha256: String,
}

#[derive(Serialize)]
pub(super) struct InjectPayload {
    pub(super) reached: bool,
}

#[derive(Serialize)]
pub(super) struct RecoverPayload {
    pub(super) outcome: &'static str,
    pub(super) acceptance: AcceptanceDocument,
    pub(super) journal: &'static str,
    pub(super) artifacts: &'static str,
    pub(super) projection: &'static str,
    pub(super) corruption: CorruptionDocument,
    pub(super) ownership: OwnershipDocument,
    pub(super) retries: RetryDocument,
    pub(super) resources: ResourceDocument,
    pub(super) temporary_objects: u16,
    pub(super) artifact_count: u16,
    pub(super) evidence: Vec<EvidenceDocument>,
    pub(super) milestones: Vec<MilestoneDocument>,
}

#[derive(Serialize)]
pub(super) struct CleanupPayload {
    pub(super) resources_released: bool,
    pub(super) owned_work_remaining: u16,
    pub(super) cleanup_steps: u16,
}

#[derive(Serialize)]
pub(super) struct AcceptanceDocument {
    pub(super) terminal: &'static str,
    pub(super) revision_bound: bool,
    pub(super) evidence_current: bool,
}

#[derive(Serialize)]
pub(super) struct CorruptionDocument {
    pub(super) detected: Option<&'static str>,
    pub(super) mutation_admitted: bool,
}

#[derive(Serialize)]
pub(super) struct OwnershipDocument {
    pub(super) scan_completed: bool,
    pub(super) discovered: u16,
    pub(super) resumed: u16,
    pub(super) failed: u16,
    pub(super) indeterminate: u16,
    pub(super) unaccounted: u16,
    pub(super) orphan_candidates_detected: u16,
    pub(super) orphans_remaining: u16,
}

#[derive(Serialize)]
pub(super) struct RetryDocument {
    pub(super) provider: u16,
    pub(super) tool: u16,
    pub(super) worker: u16,
    pub(super) reconciliation: u16,
}

#[derive(Serialize)]
pub(super) struct ResourceDocument {
    pub(super) events: u32,
    pub(super) evidence_bytes: u32,
    pub(super) peak_owned_processes: u16,
    pub(super) cleanup_steps: u16,
    pub(super) logical_ticks: u64,
}

#[derive(Serialize)]
pub(super) struct MilestoneDocument {
    pub(super) sequence: u16,
    pub(super) kind: &'static str,
    pub(super) detail: &'static str,
}

pub(super) fn publish<T: Serialize>(
    request: &BoundRequest,
    instance_id: &str,
    stage: Stage,
    payload: T,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = ResponseDocument {
        schema_version: 1,
        stage: stage.code(),
        sequence: request.sequence(),
        instance_id,
        scenario_id: request.scenario_id(),
        request_sha256: &request.request_sha256,
        payload,
    };
    let bytes = serde_json::to_vec(&response)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

pub(super) fn canonical_milestones(route: ScenarioRoute) -> Vec<MilestoneDocument> {
    let (armed, observed, reconciled) = milestone_details(route);
    vec![
        MilestoneDocument {
            sequence: 0,
            kind: "prepared",
            detail: "candidate and private daemon state prepared",
        },
        MilestoneDocument { sequence: 1, kind: "fault-armed", detail: armed },
        MilestoneDocument { sequence: 2, kind: "fault-observed", detail: observed },
        MilestoneDocument {
            sequence: 3,
            kind: "recovery-started",
            detail: "fresh candidate process opened the same journal",
        },
        MilestoneDocument { sequence: 4, kind: "reconciled", detail: reconciled },
        MilestoneDocument {
            sequence: 5,
            kind: "inspected",
            detail: "controller independently inspected retained state",
        },
    ]
}

const fn milestone_details(route: ScenarioRoute) -> (&'static str, &'static str, &'static str) {
    match route {
        ScenarioRoute::BlobBeforeDurableCommit => (
            "exact bytes held by the production artifact writer before finalization",
            "candidate killed before artifact publication",
            "artifact recovery removed the abandoned temporary bytes",
        ),
        ScenarioRoute::BlobAfterDurableCommitBeforeAck => (
            "object, metadata, and owner reference durably published",
            "candidate killed before acknowledging artifact publication",
            "exact artifact bytes and owner reference recovered",
        ),
        ScenarioRoute::BlobCorruption => (
            "finalized content-addressed bytes and their evidence reference published",
            "active object bytes changed without changing their durable identity",
            "startup quarantined divergent bytes and denied further references",
        ),
        ScenarioRoute::JournalBeforeDurableCommit => (
            "production journal append plan prepared in process memory",
            "candidate killed before submitting the append plan",
            "reopened journal has no committed event or external effect",
        ),
        ScenarioRoute::JournalAfterDurableCommitBeforeAck => (
            "durable outbox effect-before-ack fault armed",
            "candidate killed after its durable checkpoint",
            "exact effect reconciled before fence acknowledgement",
        ),
        ScenarioRoute::JournalCorruption => (
            "one valid D1 event and checkpoint committed to the authoritative journal",
            "committed event frame changed without updating its recorded digest",
            "fresh daemon startup detected divergence before authority mutation",
        ),
        ScenarioRoute::LeaseBeforeDurableCommit => (
            "move-only lease commit request prepared in process memory",
            "candidate killed before submitting the lease transition",
            "reopened journal has no lease event, head, or durable projection",
        ),
        ScenarioRoute::LeaseAfterDurableCommitBeforeAck => (
            "lease transition and projection durably committed together",
            "candidate killed before acknowledging the committed lease receipt",
            "exact lease event, head, revision, digest, and producer reopened",
        ),
        ScenarioRoute::GateBeforeDurableCommit => (
            "production D1 start transition accepted in process memory",
            "candidate killed before submitting the gate transition",
            "gate journal and complete checkpoint remained absent",
        ),
        ScenarioRoute::GateAfterDurableCommitBeforeAck => (
            "gate event and complete D1 checkpoint committed atomically",
            "candidate killed before acknowledging the gate commit receipt",
            "exact gate plan, successor state, event, and checkpoint reopened",
        ),
        ScenarioRoute::PatchBeforeDurableCommit => (
            "workspace-bound production patch plan prepared in process memory",
            "candidate killed before creating transaction state or changing the target",
            "workspace reopened without the target or pending transaction metadata",
        ),
        ScenarioRoute::PatchAfterDurableCommitBeforeAck => (
            "production patch postimage and transaction receipt durably completed",
            "candidate killed before acknowledging the applied patch receipt",
            "exact target bytes reopened with no pending transaction metadata",
        ),
        ScenarioRoute::SnapshotBeforeDurableCommit => (
            "production candidate tree prepared before snapshot publication",
            "candidate killed before creating the snapshot commit and retained ref",
            "repository reopened with no retained snapshot reference",
        ),
        ScenarioRoute::SnapshotAfterDurableCommitBeforeAck => (
            "synthetic snapshot commit and retained ref durably published",
            "candidate killed before acknowledging snapshot publication",
            "exact snapshot manifest, commit, tree, and retained ref reopened",
        ),
        ScenarioRoute::SnapshotCorruption => (
            "synthetic snapshot commit, manifest, and retained ref published",
            "active snapshot ref redirected to a different repository commit",
            "fresh recovery atomically moved the divergent ref into quarantine",
        ),
        ScenarioRoute::PromotionBeforeDurableCommit => (
            "production promotion transitions accepted with approve-once authority",
            "candidate killed before submitting the atomic activation",
            "campaign, production pointer, and approval remained at their predecessors",
        ),
        ScenarioRoute::PromotionAfterDurableCommitBeforeAck => (
            "campaign, pointer, checkpoints, and approval consumption committed atomically",
            "candidate killed before acknowledging the promotion receipt",
            "exact promoted campaign, active pointer, and consumed approval reopened",
        ),
        ScenarioRoute::ProjectionCorruption => (
            "active journal projection generation installed and checksum-verified",
            "active projection payload bytes replaced without changing their recorded digest",
            "startup replay installed and verified a new atomic shadow generation",
        ),
    }
}
