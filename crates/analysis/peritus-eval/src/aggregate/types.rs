//! Compact durable campaign value records.

use peritus_artifact_store::ArtifactDigest;
use peritus_scheduler::WorkId;
use peritus_types::{EvidenceId, Sha256Digest};

use crate::{
    EvaluationError, EvaluationErrorKind, EvaluationOperation, EvaluationPlanId,
    EvaluationRecovery, EvaluationReportId, PlanDigest, RolloutId,
};

/// Closed durable campaign phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvaluationPhase {
    /// Immutable campaign inputs are registered.
    Created,
    /// Complete plan is committed.
    Planned,
    /// D3 scheduling directives are in flight.
    Scheduling,
    /// At least one rollout is executing or settled.
    Running,
    /// Durable cancellation is being reconciled.
    Cancelling,
    /// Complete ledger is under deterministic analysis.
    Analyzing,
    /// Canonical report is committed and awaits publication.
    ReportReady,
    /// Evidence-backed publication completed.
    Published,
    /// Typed terminal failure won.
    Failed,
    /// Cancellation completed for every unsettled rollout.
    Cancelled,
}

impl EvaluationPhase {
    /// Returns whether no later success may replace this phase.
    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(self, Self::Published | Self::Failed | Self::Cancelled)
    }
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Created => 1,
            Self::Planned => 2,
            Self::Scheduling => 3,
            Self::Running => 4,
            Self::Cancelling => 5,
            Self::Analyzing => 6,
            Self::ReportReady => 7,
            Self::Published => 8,
            Self::Failed => 9,
            Self::Cancelled => 10,
        }
    }
    pub(crate) const fn from_tag(tag: u8) -> Result<Self, EvaluationError> {
        match tag {
            1 => Ok(Self::Created),
            2 => Ok(Self::Planned),
            3 => Ok(Self::Scheduling),
            4 => Ok(Self::Running),
            5 => Ok(Self::Cancelling),
            6 => Ok(Self::Analyzing),
            7 => Ok(Self::ReportReady),
            8 => Ok(Self::Published),
            9 => Ok(Self::Failed),
            10 => Ok(Self::Cancelled),
            _ => Err(protocol("unknown evaluation phase tag")),
        }
    }
}

/// One rollout binding retained in a bounded plan shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedRolloutBinding {
    rollout_id: RolloutId,
    work_id: WorkId,
    request_digest: Sha256Digest,
}

impl PlannedRolloutBinding {
    /// Creates one exact planned binding.
    #[must_use]
    pub const fn new(rollout_id: RolloutId, work_id: WorkId, request_digest: Sha256Digest) -> Self {
        Self { rollout_id, work_id, request_digest }
    }
    /// Rollout identity.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// D3 work identity.
    #[must_use]
    pub const fn work_id(self) -> WorkId {
        self.work_id
    }
    /// Complete execution request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
}

/// One bounded canonical plan artifact shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanBatch {
    ordinal: u32,
    total_batches: u32,
    artifact: ArtifactDigest,
    bindings: Vec<PlannedRolloutBinding>,
}

impl PlanBatch {
    /// Creates a nonempty canonical plan batch.
    ///
    /// # Errors
    /// Rejects zero/gapped metadata, empty rows, duplicate IDs, or noncanonical order.
    pub fn new(
        ordinal: u32,
        total_batches: u32,
        artifact: ArtifactDigest,
        bindings: Vec<PlannedRolloutBinding>,
    ) -> Result<Self, EvaluationError> {
        if ordinal == 0
            || total_batches == 0
            || ordinal > total_batches
            || bindings.is_empty()
            || bindings.windows(2).any(|pair| pair[0].rollout_id() >= pair[1].rollout_id())
        {
            return Err(invalid("plan batch metadata or rollout order is invalid"));
        }
        Ok(Self { ordinal, total_batches, artifact, bindings })
    }
    /// One-based batch ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// Frozen total batch count.
    #[must_use]
    pub const fn total_batches(&self) -> u32 {
        self.total_batches
    }
    /// Exact finalized shard artifact.
    #[must_use]
    pub const fn artifact(&self) -> ArtifactDigest {
        self.artifact
    }
    /// Canonical rollout bindings.
    #[must_use]
    pub fn bindings(&self) -> &[PlannedRolloutBinding] {
        &self.bindings
    }
}

/// Complete immutable plan root and cardinality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanRecord {
    id: EvaluationPlanId,
    digest: PlanDigest,
    root: ArtifactDigest,
    expected_rollouts: u32,
    total_batches: u32,
}

impl PlanRecord {
    /// Creates one nonempty complete plan record.
    ///
    /// # Errors
    /// Rejects zero rollout/batch cardinality.
    pub const fn new(
        id: EvaluationPlanId,
        digest: PlanDigest,
        root: ArtifactDigest,
        expected_rollouts: u32,
        total_batches: u32,
    ) -> Result<Self, EvaluationError> {
        if expected_rollouts == 0 || total_batches == 0 {
            Err(invalid("complete plan cardinality is zero"))
        } else {
            Ok(Self { id, digest, root, expected_rollouts, total_batches })
        }
    }
    /// Plan identity.
    #[must_use]
    pub const fn id(self) -> EvaluationPlanId {
        self.id
    }
    /// Plan digest.
    #[must_use]
    pub const fn digest(self) -> PlanDigest {
        self.digest
    }
    /// Root manifest artifact.
    #[must_use]
    pub const fn root(self) -> ArtifactDigest {
        self.root
    }
    /// Complete logical rollout cardinality.
    #[must_use]
    pub const fn expected_rollouts(self) -> u32 {
        self.expected_rollouts
    }
    /// Complete plan shard count.
    #[must_use]
    pub const fn total_batches(self) -> u32 {
        self.total_batches
    }
}

/// Compact per-rollout state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RolloutStatus {
    /// Planned but no schedule directive committed.
    Planned,
    /// Exact schedule directive is outstanding.
    Scheduling,
    /// D3 acknowledged exact work identity.
    Scheduled {
        /// Exact D3 acknowledgement digest.
        acknowledgement_digest: Sha256Digest,
    },
    /// Attempt start was durably committed before external I/O.
    Running {
        /// One-based durably started attempt.
        attempt: u16,
    },
    /// Logical terminal record was committed.
    Settled(TerminalRecordRef),
    /// Cancellation won before a logical task verdict.
    Cancelled {
        /// Durable campaign cancellation reason digest.
        reason_digest: Sha256Digest,
        /// Exact local or external cancellation settlement observation.
        observation_digest: Sha256Digest,
    },
}

/// Compact rollout binding and progress checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutProgress {
    binding: PlannedRolloutBinding,
    status: RolloutStatus,
    attempts_retained: u16,
}

impl RolloutProgress {
    pub(crate) const fn planned(binding: PlannedRolloutBinding) -> Self {
        Self { binding, status: RolloutStatus::Planned, attempts_retained: 0 }
    }
    /// Immutable planned binding.
    #[must_use]
    pub const fn binding(self) -> PlannedRolloutBinding {
        self.binding
    }
    /// Current durable status.
    #[must_use]
    pub const fn status(self) -> RolloutStatus {
        self.status
    }
    /// Number of attempts whose evidence was retained.
    #[must_use]
    pub const fn attempts_retained(self) -> u16 {
        self.attempts_retained
    }
    pub(crate) const fn set_status(&mut self, status: RolloutStatus) {
        self.status = status;
    }
    pub(crate) const fn retain_attempt(&mut self, attempt: u16) {
        self.attempts_retained = attempt;
    }
    pub(crate) const fn decoded(
        binding: PlannedRolloutBinding,
        status: RolloutStatus,
        attempts_retained: u16,
    ) -> Self {
        Self { binding, status, attempts_retained }
    }
}

/// Compact terminal class used for progress/count projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RolloutTerminalClass {
    /// Evaluator-confirmed pass.
    Passed,
    /// Evaluator-confirmed task failure.
    TaskFailed,
    /// Infrastructure prevented a valid task verdict.
    InfrastructureFailed,
    /// External result remained ambiguous.
    Ambiguous,
}

impl RolloutTerminalClass {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Passed => 1,
            Self::TaskFailed => 2,
            Self::InfrastructureFailed => 3,
            Self::Ambiguous => 4,
        }
    }
    pub(crate) const fn from_tag(tag: u8) -> Result<Self, EvaluationError> {
        match tag {
            1 => Ok(Self::Passed),
            2 => Ok(Self::TaskFailed),
            3 => Ok(Self::InfrastructureFailed),
            4 => Ok(Self::Ambiguous),
            _ => Err(protocol("unknown rollout terminal class")),
        }
    }
}

/// Artifact-backed logical terminal reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalRecordRef {
    class: RolloutTerminalClass,
    record_digest: Sha256Digest,
    artifact: ArtifactDigest,
    artifact_bytes: u64,
    attempt: u16,
}

impl TerminalRecordRef {
    /// Creates one complete terminal reference.
    ///
    /// # Errors
    /// Rejects zero byte length or attempt.
    pub const fn new(
        class: RolloutTerminalClass,
        record_digest: Sha256Digest,
        artifact: ArtifactDigest,
        artifact_bytes: u64,
        attempt: u16,
    ) -> Result<Self, EvaluationError> {
        if artifact_bytes == 0 || attempt == 0 {
            Err(invalid("terminal record reference has zero size or attempt"))
        } else {
            Ok(Self { class, record_digest, artifact, artifact_bytes, attempt })
        }
    }
    /// Terminal classification.
    #[must_use]
    pub const fn class(self) -> RolloutTerminalClass {
        self.class
    }
    /// Complete semantic record digest.
    #[must_use]
    pub const fn record_digest(self) -> Sha256Digest {
        self.record_digest
    }
    /// Exact result artifact.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Exact result bytes.
    #[must_use]
    pub const fn artifact_bytes(self) -> u64 {
        self.artifact_bytes
    }
    /// Settled attempt.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
}

/// Canonical report artifact record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReportRecord {
    id: EvaluationReportId,
    payload_digest: Sha256Digest,
    artifact: ArtifactDigest,
    size: u64,
}

impl ReportRecord {
    /// Creates an exact nonempty report artifact record.
    ///
    /// # Errors
    /// Rejects a zero-length report artifact.
    pub const fn new(
        id: EvaluationReportId,
        payload_digest: Sha256Digest,
        artifact: ArtifactDigest,
        size: u64,
    ) -> Result<Self, EvaluationError> {
        if size == 0 {
            Err(invalid("report artifact size is zero"))
        } else {
            Ok(Self { id, payload_digest, artifact, size })
        }
    }
    /// Report identity.
    #[must_use]
    pub const fn id(self) -> EvaluationReportId {
        self.id
    }
    /// Canonical report payload digest.
    #[must_use]
    pub const fn payload_digest(self) -> Sha256Digest {
        self.payload_digest
    }
    /// Finalized report artifact.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Exact report byte length.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
}

/// Evidence-backed publication record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationRecord {
    report_id: EvaluationReportId,
    evidence_id: EvidenceId,
    report_commit_position: u64,
}

impl PublicationRecord {
    /// Creates one nonzero publication provenance record.
    ///
    /// # Errors
    /// Rejects a zero journal position because it cannot identify a committed report event.
    pub const fn new(
        report_id: EvaluationReportId,
        evidence_id: EvidenceId,
        report_commit_position: u64,
    ) -> Result<Self, EvaluationError> {
        if report_commit_position == 0 {
            Err(invalid("publication commit position is zero"))
        } else {
            Ok(Self { report_id, evidence_id, report_commit_position })
        }
    }
    /// Published report.
    #[must_use]
    pub const fn report_id(self) -> EvaluationReportId {
        self.report_id
    }
    /// Admitted evidence identity.
    #[must_use]
    pub const fn evidence_id(self) -> EvidenceId {
        self.evidence_id
    }
    /// Report event journal position.
    #[must_use]
    pub const fn report_commit_position(self) -> u64 {
        self.report_commit_position
    }
}

/// Stable terminal campaign failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CampaignFailureCode {
    /// Plan or profile binding failed.
    Binding,
    /// Durable accounting could not be reconciled.
    Accounting,
    /// Analysis failed after complete settlement.
    Analysis,
    /// Artifact or evidence publication failed terminally.
    Publication,
    /// Authoritative state was corrupt.
    Corruption,
}

impl CampaignFailureCode {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Binding => 1,
            Self::Accounting => 2,
            Self::Analysis => 3,
            Self::Publication => 4,
            Self::Corruption => 5,
        }
    }
    pub(crate) const fn from_tag(tag: u8) -> Result<Self, EvaluationError> {
        match tag {
            1 => Ok(Self::Binding),
            2 => Ok(Self::Accounting),
            3 => Ok(Self::Analysis),
            4 => Ok(Self::Publication),
            5 => Ok(Self::Corruption),
            _ => Err(protocol("unknown campaign failure code")),
        }
    }
}

/// Redaction-safe terminal campaign failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CampaignFailure {
    code: CampaignFailureCode,
    digest: Sha256Digest,
}

impl CampaignFailure {
    /// Creates one typed digest-bound failure.
    #[must_use]
    pub const fn new(code: CampaignFailureCode, digest: Sha256Digest) -> Self {
        Self { code, digest }
    }
    /// Stable failure class.
    #[must_use]
    pub const fn code(self) -> CampaignFailureCode {
        self.code
    }
    /// Exact bounded failure record digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

const fn invalid(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::ApplyTransition,
        EvaluationRecovery::CorrectInput,
        detail,
    )
}

const fn protocol(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
