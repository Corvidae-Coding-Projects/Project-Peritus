//! Compact durable campaign value records.

use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_scheduler::WorkId;
use peritus_types::{ActorId, EvidenceId, Sha256Digest};

use crate::{
    DatasetDigest, EvaluationError, EvaluationErrorKind, EvaluationOperation, EvaluationPlanId,
    EvaluationRecovery, EvaluationReportId, EvaluationRetryPolicy, PlanDigest, ProfileDigest,
    ResultDigest, RolloutId, RolloutOutcome, RolloutRecord,
};

const RETRY_INTENT_DOMAIN: &[u8] = b"peritus.evaluation.retry-intent.v1\0";

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
    /// Work is durably stopped at an explicit resumable boundary.
    Suspended,
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
            Self::Suspended => 11,
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
            11 => Ok(Self::Suspended),
            _ => Err(protocol("unknown evaluation phase tag")),
        }
    }
}

/// Artifact-backed resumable analysis boundary owned by one exact actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisSafePoint {
    owner: ActorId,
    sequence: u64,
    checkpoint_digest: Sha256Digest,
    artifact: ArtifactDigest,
    artifact_bytes: u64,
}

impl AnalysisSafePoint {
    /// Creates one nonempty monotonic analysis checkpoint.
    ///
    /// # Errors
    /// Rejects a zero sequence or empty checkpoint artifact.
    pub const fn new(
        owner: ActorId,
        sequence: u64,
        checkpoint_digest: Sha256Digest,
        artifact: ArtifactDigest,
        artifact_bytes: u64,
    ) -> Result<Self, EvaluationError> {
        if sequence == 0 || artifact_bytes == 0 {
            Err(invalid("analysis safe point has zero sequence or artifact size"))
        } else {
            Ok(Self { owner, sequence, checkpoint_digest, artifact, artifact_bytes })
        }
    }
    /// Actor that exclusively owns continuation from this boundary.
    #[must_use]
    pub const fn owner(self) -> ActorId {
        self.owner
    }
    /// One-based monotonic checkpoint sequence within the owner.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Digest of the complete resumable analysis state.
    #[must_use]
    pub const fn checkpoint_digest(self) -> Sha256Digest {
        self.checkpoint_digest
    }
    /// Finalized retained checkpoint artifact.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Exact retained checkpoint byte length.
    #[must_use]
    pub const fn artifact_bytes(self) -> u64 {
        self.artifact_bytes
    }
}

/// Artifact-backed evidence retained from one retryable execution attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryAttemptRecord {
    rollout_id: RolloutId,
    attempt: u16,
    observation_digest: Sha256Digest,
    record_digest: Sha256Digest,
    artifact: ArtifactDigest,
    artifact_bytes: u64,
    continuation_digest: Option<Sha256Digest>,
}

impl RetryAttemptRecord {
    /// Creates one complete retained retryable-attempt record.
    ///
    /// # Errors
    /// Rejects attempt zero or an empty retained artifact.
    pub(crate) const fn new(
        rollout_id: RolloutId,
        attempt: u16,
        observation_digest: Sha256Digest,
        record_digest: Sha256Digest,
        artifact: ArtifactDigest,
        artifact_bytes: u64,
        continuation_digest: Option<Sha256Digest>,
    ) -> Result<Self, EvaluationError> {
        if attempt == 0 || artifact_bytes == 0 {
            Err(invalid("retryable attempt record has zero attempt or artifact size"))
        } else {
            Ok(Self {
                rollout_id,
                attempt,
                observation_digest,
                record_digest,
                artifact,
                artifact_bytes,
                continuation_digest,
            })
        }
    }
    /// Binds a checked retryable rollout record to its finalized retained artifact.
    ///
    /// # Errors
    /// Rejects a nonretryable logical outcome or an empty retained artifact.
    pub fn from_rollout_record(
        record: RolloutRecord,
        artifact: ArtifactDigest,
        artifact_bytes: u64,
    ) -> Result<Self, EvaluationError> {
        if !matches!(
            record.outcome(),
            RolloutOutcome::InfrastructureFailed { retryable: true, .. }
        ) {
            return Err(invalid("retained retry record does not contain a retryable outcome"));
        }
        Self::new(
            record.rollout_id(),
            record.attempt().number(),
            record.attempt().observation_digest(),
            record.digest(),
            artifact,
            artifact_bytes,
            record.continuation().map(crate::ExecutionContinuation::digest),
        )
    }
    /// Logical rollout whose attempt evidence was retained.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Retained completed attempt.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Complete retained observation digest.
    #[must_use]
    pub const fn observation_digest(self) -> Sha256Digest {
        self.observation_digest
    }
    /// Complete semantic retained rollout-record digest.
    #[must_use]
    pub const fn record_digest(self) -> Sha256Digest {
        self.record_digest
    }
    /// Exact retained attempt artifact.
    #[must_use]
    pub const fn artifact(self) -> ArtifactDigest {
        self.artifact
    }
    /// Exact retained attempt artifact byte length.
    #[must_use]
    pub const fn artifact_bytes(self) -> u64 {
        self.artifact_bytes
    }
    /// Exact same-stage continuation identity retained by the execution owner.
    #[must_use]
    pub const fn continuation_digest(self) -> Option<Sha256Digest> {
        self.continuation_digest
    }
}

/// Frozen retry decision that names the exact next attempt and deterministic backoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryIntent {
    retained: RetryAttemptRecord,
    next_attempt: u16,
    backoff_micros: u64,
    profile_digest: ProfileDigest,
    policy: EvaluationRetryPolicy,
}

impl RetryIntent {
    /// Derives a retry from retained evidence and the exact frozen caller policy.
    ///
    /// # Errors
    /// Rejects an exhausted finite policy or exhausted attempt-number representation.
    pub(crate) fn new(
        retained: RetryAttemptRecord,
        profile_digest: ProfileDigest,
        policy: EvaluationRetryPolicy,
    ) -> Result<Self, EvaluationError> {
        let (next_attempt, backoff_micros) = policy
            .next_retry(retained.attempt())
            .ok_or_else(|| invalid("retry intent exceeds the frozen stopping policy"))?;
        Ok(Self { retained, next_attempt, backoff_micros, profile_digest, policy })
    }
    /// Retained evidence from the completed retryable attempt.
    #[must_use]
    pub const fn retained(self) -> RetryAttemptRecord {
        self.retained
    }
    /// Exact next attempt to execute.
    #[must_use]
    pub const fn next_attempt(self) -> u16 {
        self.next_attempt
    }
    /// Frozen deterministic delay before the next attempt is eligible.
    #[must_use]
    pub const fn backoff_micros(self) -> u64 {
        self.backoff_micros
    }
    /// Frozen evaluation profile that owns this retry policy.
    #[must_use]
    pub const fn profile_digest(self) -> ProfileDigest {
        self.profile_digest
    }
    /// Exact finite or persistent caller retry policy.
    #[must_use]
    pub const fn policy(self) -> EvaluationRetryPolicy {
        self.policy
    }

    pub(crate) fn canonical_bytes(self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(RETRY_INTENT_DOMAIN).map_err(retry_codec)?;
        writer.write_fixed(self.retained.rollout_id().as_bytes()).map_err(retry_codec)?;
        writer.write_u16(self.retained.attempt()).map_err(retry_codec)?;
        writer
            .write_fixed(self.retained.observation_digest().as_bytes())
            .map_err(retry_codec)?;
        writer.write_fixed(self.retained.record_digest().as_bytes()).map_err(retry_codec)?;
        writer.write_fixed(self.retained.artifact().as_bytes()).map_err(retry_codec)?;
        writer.write_u64(self.retained.artifact_bytes()).map_err(retry_codec)?;
        writer
            .write_option_tag(self.retained.continuation_digest().is_some())
            .map_err(retry_codec)?;
        if let Some(value) = self.retained.continuation_digest() {
            writer.write_fixed(value.as_bytes()).map_err(retry_codec)?;
        }
        writer.write_u16(self.next_attempt).map_err(retry_codec)?;
        writer.write_u64(self.backoff_micros).map_err(retry_codec)?;
        writer.write_fixed(self.profile_digest.as_bytes()).map_err(retry_codec)?;
        writer.write_option_tag(self.policy.stop_after_attempt().is_some()).map_err(retry_codec)?;
        if let Some(value) = self.policy.stop_after_attempt() {
            writer.write_u16(value).map_err(retry_codec)?;
        }
        writer.write_u64(self.policy.initial_backoff_micros()).map_err(retry_codec)?;
        writer.write_u64(self.policy.maximum_backoff_micros()).map_err(retry_codec)?;
        Ok(writer.into_bytes())
    }

    pub(crate) fn decode_canonical(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(retry_codec)? != RETRY_INTENT_DOMAIN {
            return Err(protocol("unsupported retry-intent domain"));
        }
        let rollout_id = RolloutId::new(reader.read_fixed().map_err(retry_codec)?)?;
        let attempt = reader.read_u16().map_err(retry_codec)?;
        let observation_digest = Sha256Digest::new(reader.read_fixed().map_err(retry_codec)?);
        let record_digest = Sha256Digest::new(reader.read_fixed().map_err(retry_codec)?);
        let artifact = ArtifactDigest::from_sha256(Sha256Digest::new(
            reader.read_fixed().map_err(retry_codec)?,
        ));
        let artifact_bytes = reader.read_u64().map_err(retry_codec)?;
        let continuation_digest = reader
            .read_option_tag()
            .map_err(retry_codec)?
            .then(|| {
                Ok::<_, EvaluationError>(Sha256Digest::new(
                    reader.read_fixed().map_err(retry_codec)?,
                ))
            })
            .transpose()?;
        let encoded_next_attempt = reader.read_u16().map_err(retry_codec)?;
        let encoded_backoff_micros = reader.read_u64().map_err(retry_codec)?;
        let profile_digest = ProfileDigest::new(Sha256Digest::new(
            reader.read_fixed().map_err(retry_codec)?,
        ));
        let stop_after_attempt = reader
            .read_option_tag()
            .map_err(retry_codec)?
            .then(|| reader.read_u16().map_err(retry_codec))
            .transpose()?;
        let policy = EvaluationRetryPolicy::from_canonical(
            stop_after_attempt,
            reader.read_u64().map_err(retry_codec)?,
            reader.read_u64().map_err(retry_codec)?,
        )?;
        reader.finish().map_err(retry_codec)?;
        let retained = RetryAttemptRecord::new(
            rollout_id,
            attempt,
            observation_digest,
            record_digest,
            artifact,
            artifact_bytes,
            continuation_digest,
        )?;
        let intent = Self::new(retained, profile_digest, policy)?;
        if intent.next_attempt != encoded_next_attempt
            || intent.backoff_micros != encoded_backoff_micros
        {
            return Err(protocol("retry intent differs from its frozen policy"));
        }
        Ok(intent)
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
    /// A retained retry is durably paired with its exact outstanding execution directive.
    RetryPending {
        /// Complete retained evidence, policy, delay, and next-attempt identity.
        retry: RetryIntent,
    },
    /// The exact retained retry attempt was committed before its external effect began.
    RetryRunning {
        /// Complete retained evidence, policy, delay, and active attempt identity.
        retry: RetryIntent,
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

/// Complete semantic and artifact contract accepted for one report completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisReportBinding {
    report: ReportRecord,
    dataset_digest: DatasetDigest,
    profile_digest: ProfileDigest,
    plan_id: EvaluationPlanId,
    plan_digest: PlanDigest,
    analysis_digest: ResultDigest,
    analysis_artifact: ArtifactDigest,
    analysis_artifact_bytes: u64,
}

impl AnalysisReportBinding {
    /// Creates one complete nonempty analysis-to-report contract.
    ///
    /// # Errors
    /// Rejects a zero-length analysis artifact.
    #[allow(clippy::too_many_arguments, reason = "every immutable report binding stays explicit")]
    pub const fn new(
        report: ReportRecord,
        dataset_digest: DatasetDigest,
        profile_digest: ProfileDigest,
        plan_id: EvaluationPlanId,
        plan_digest: PlanDigest,
        analysis_digest: ResultDigest,
        analysis_artifact: ArtifactDigest,
        analysis_artifact_bytes: u64,
    ) -> Result<Self, EvaluationError> {
        if analysis_artifact_bytes == 0 {
            Err(invalid("analysis-to-report artifact size is zero"))
        } else {
            Ok(Self {
                report,
                dataset_digest,
                profile_digest,
                plan_id,
                plan_digest,
                analysis_digest,
                analysis_artifact,
                analysis_artifact_bytes,
            })
        }
    }
    /// Canonical report artifact record.
    #[must_use]
    pub const fn report(self) -> ReportRecord {
        self.report
    }
    /// Frozen dataset used by the analysis and report.
    #[must_use]
    pub const fn dataset_digest(self) -> DatasetDigest {
        self.dataset_digest
    }
    /// Frozen profile used by the analysis and report.
    #[must_use]
    pub const fn profile_digest(self) -> ProfileDigest {
        self.profile_digest
    }
    /// Complete plan used by the analysis and report.
    #[must_use]
    pub const fn plan_id(self) -> EvaluationPlanId {
        self.plan_id
    }
    /// Complete plan digest used by the analysis and report.
    #[must_use]
    pub const fn plan_digest(self) -> PlanDigest {
        self.plan_digest
    }
    /// Semantic digest of the complete analysis.
    #[must_use]
    pub const fn analysis_digest(self) -> ResultDigest {
        self.analysis_digest
    }
    /// Finalized canonical analysis artifact.
    #[must_use]
    pub const fn analysis_artifact(self) -> ArtifactDigest {
        self.analysis_artifact
    }
    /// Exact canonical analysis artifact length.
    #[must_use]
    pub const fn analysis_artifact_bytes(self) -> u64 {
        self.analysis_artifact_bytes
    }
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

/// Exact retained outcome when cancellation wins a report-publication race.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationCancellationRecord {
    report: ReportRecord,
    observation_digest: Sha256Digest,
    admitted_evidence: Option<EvidenceId>,
}

impl PublicationCancellationRecord {
    /// Creates an exact cancellation observation for a committed report.
    #[must_use]
    pub const fn new(
        report: ReportRecord,
        observation_digest: Sha256Digest,
        admitted_evidence: Option<EvidenceId>,
    ) -> Self {
        Self { report, observation_digest, admitted_evidence }
    }
    /// Complete report that was prevented from becoming published state.
    #[must_use]
    pub const fn report(self) -> ReportRecord {
        self.report
    }
    /// External publication-cancellation observation.
    #[must_use]
    pub const fn observation_digest(self) -> Sha256Digest {
        self.observation_digest
    }
    /// Evidence already admitted before cancellation won, when present.
    #[must_use]
    pub const fn admitted_evidence(self) -> Option<EvidenceId> {
        self.admitted_evidence
    }
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

const fn retry_codec(_: peritus_codec::CodecError) -> EvaluationError {
    protocol("retry intent violates canonical codec bounds")
}
