//! Complete immutable logical-terminal records.

use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CanonicalReader, CodecLimits};
use peritus_scheduler::WorkId;
use peritus_types::Sha256Digest;

use crate::{
    EvaluationArm, EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    ExecutedRollout, ProfileDigest, ResourceObservation, RolloutAttempt, RolloutId, RolloutOutcome,
    RolloutSeed, RolloutSpec, TaskId,
};

const RECORD_DOMAIN: &[u8] = b"peritus.evaluation.rollout-record.v2\0";

/// Complete result retained for one expected logical rollout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutRecord {
    rollout_id: RolloutId,
    campaign_id: EvaluationCampaignId,
    profile_digest: ProfileDigest,
    task_id: TaskId,
    arm: EvaluationArm,
    ordinal: u16,
    seed: RolloutSeed,
    work_id: WorkId,
    request_digest: Sha256Digest,
    attempt: RolloutAttempt,
    candidate_output: Option<ArtifactDigest>,
    candidate_resources: Option<ResourceObservation>,
    evaluator_resources: Option<ResourceObservation>,
    trace_root: Option<ArtifactDigest>,
    evidence_root: Option<ArtifactDigest>,
    digest: Sha256Digest,
}

impl RolloutRecord {
    /// Binds a checked execution result to every frozen rollout identity.
    ///
    /// # Errors
    /// Rejects cross-rollout observations or inconsistent terminal evidence.
    pub fn from_execution(
        spec: &RolloutSpec,
        executed: ExecutedRollout,
        trace_root: Option<ArtifactDigest>,
        evidence_root: Option<ArtifactDigest>,
    ) -> Result<Self, EvaluationError> {
        if executed.candidate().is_some_and(|value| value.rollout_id() != spec.id())
            || executed.evaluator().is_some_and(|value| value.rollout_id() != spec.id())
            || executed.attempt().number() == 0
        {
            return Err(invalid("terminal observations differ from the rollout identity"));
        }
        let candidate_output = executed.candidate().map(crate::CandidateObservation::output);
        let candidate_resources = executed.candidate().map(crate::CandidateObservation::resources);
        let evaluator_resources = executed.evaluator().map(crate::EvaluatorObservation::resources);
        if executed.evaluator().is_some() && candidate_output.is_none() {
            return Err(invalid("evaluator observation exists without candidate output"));
        }
        let mut record = Self {
            rollout_id: spec.id(),
            campaign_id: spec.campaign_id(),
            profile_digest: spec.profile_digest(),
            task_id: spec.task_id(),
            arm: spec.arm(),
            ordinal: spec.ordinal(),
            seed: spec.seed(),
            work_id: spec.work_id(),
            request_digest: spec.request_digest(),
            attempt: executed.attempt(),
            candidate_output,
            candidate_resources,
            evaluator_resources,
            trace_root,
            evidence_root,
            digest: Sha256Digest::new([0; 32]),
        };
        record.digest = peritus_codec::sha256(&record.identity_bytes());
        Ok(record)
    }

    /// Retains an ambiguous interrupted attempt from the exact owned-process recovery observation.
    ///
    /// This is an inert record constructor: callers must first reconcile the actual process owner,
    /// retain its recovery report, and establish that no old process tree can continue. An interrupted
    /// attempt supplies no evaluator verdict or completed stage observations. The cancellation flag
    /// must come from the durable campaign state, rather than being guessed during recovery.
    ///
    /// # Errors
    /// Rejects attempt zero.
    pub fn from_interruption(
        spec: &RolloutSpec,
        attempt: u16,
        recovery_digest: Sha256Digest,
        late_after_cancellation: bool,
    ) -> Result<Self, EvaluationError> {
        Ok(Self::from_unobserved_attempt(
            spec,
            RolloutAttempt::new(
                attempt,
                recovery_digest,
                RolloutOutcome::Ambiguous { observation_digest: recovery_digest },
                late_after_cancellation,
            )?,
        ))
    }

    /// Retains an actual execution failure without inventing a completed stage observation.
    ///
    /// The caller must retain the exact diagnostic and reconcile actual process effects and teardown
    /// before constructing this inert observation. A diagnostic may retain a known failed native
    /// terminal and its physical outputs or measurements. Stage observations unavailable to this
    /// record stay absent; an execution failure never supplies a task verdict.
    ///
    /// # Errors
    /// Rejects attempt zero or a failure from a layer other than candidate execution.
    pub fn from_execution_failure(
        spec: &RolloutSpec,
        attempt: u16,
        failure: crate::ExecutionFailure,
        late_after_cancellation: bool,
    ) -> Result<Self, EvaluationError> {
        if failure.class() != crate::InfrastructureFailureClass::Execution {
            return Err(invalid("unobserved execution failure belongs to another layer"));
        }
        Ok(Self::from_unobserved_attempt(
            spec,
            RolloutAttempt::new(
                attempt,
                failure.digest(),
                failure.outcome(),
                late_after_cancellation,
            )?,
        ))
    }

    fn from_unobserved_attempt(spec: &RolloutSpec, attempt: RolloutAttempt) -> Self {
        let mut record = Self {
            rollout_id: spec.id(),
            campaign_id: spec.campaign_id(),
            profile_digest: spec.profile_digest(),
            task_id: spec.task_id(),
            arm: spec.arm(),
            ordinal: spec.ordinal(),
            seed: spec.seed(),
            work_id: spec.work_id(),
            request_digest: spec.request_digest(),
            attempt,
            candidate_output: None,
            candidate_resources: None,
            evaluator_resources: None,
            trace_root: None,
            evidence_root: None,
            digest: Sha256Digest::new([0; 32]),
        };
        record.digest = peritus_codec::sha256(&record.identity_bytes());
        record
    }

    /// Retains an actual late observation without changing its outcome or completed stage facts.
    ///
    /// The serialized owner calls this only after observing durable cancellation. This inert
    /// marker does not replace the logical cancellation or confer candidate authority.
    ///
    /// # Errors
    /// Rejects an invalid retained attempt before recomputing the complete version-two digest.
    pub fn mark_late_after_cancellation(mut self) -> Result<Self, EvaluationError> {
        self.attempt = RolloutAttempt::new(
            self.attempt.number(),
            self.attempt.observation_digest(),
            self.attempt.terminal(),
            true,
        )?;
        self.digest = peritus_codec::sha256(&self.identity_bytes());
        Ok(self)
    }

    /// Binds an actual finalized trace root without changing any retained execution observation.
    ///
    /// The caller must obtain this root from its trace owner. This inert binding grants neither
    /// trace publication nor execution authority.
    #[must_use]
    pub fn with_trace_root(mut self, trace_root: ArtifactDigest) -> Self {
        self.trace_root = Some(trace_root);
        self.digest = peritus_codec::sha256(&self.identity_bytes());
        self
    }

    /// Canonically encodes every retained field and the advertised record digest.
    #[must_use]
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = self.identity_bytes();
        bytes.extend_from_slice(self.digest.as_bytes());
        bytes
    }

    /// Recovers a checked terminal record without supplying execution authority.
    ///
    /// Unknown resource observations remain unknown. Version-one identities did not retain the
    /// late-after-cancellation flag and are rejected rather than inventing that observation.
    ///
    /// # Errors
    /// Rejects unsupported domains, malformed tags, truncated or trailing bytes, invalid derived
    /// identities, inconsistent stage observations, and an incorrect record digest.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_fixed::<{ RECORD_DOMAIN.len() }>().map_err(codec)? != RECORD_DOMAIN {
            return Err(corrupt("unsupported rollout record domain"));
        }
        let rollout_id = RolloutId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid rollout record identity"))?;
        let campaign_id = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid rollout record campaign identity"))?;
        let profile_digest = ProfileDigest::new(read_digest(&mut reader)?);
        let task_id = TaskId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid rollout record task identity"))?;
        let arm = match reader.read_u8().map_err(codec)? {
            1 => EvaluationArm::Baseline,
            2 => EvaluationArm::Candidate,
            _ => return Err(corrupt("invalid rollout record arm")),
        };
        let ordinal = reader.read_u16().map_err(codec)?;
        if ordinal == 0 {
            return Err(corrupt("rollout record ordinal is zero"));
        }
        let seed = RolloutSeed::derive(profile_digest, task_id, ordinal);
        if reader.read_fixed::<32>().map_err(codec)? != *seed.as_bytes() {
            return Err(corrupt("rollout record paired seed differs"));
        }
        let work_id = WorkId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid rollout record work identity"))?;
        let request_digest = read_digest(&mut reader)?;
        let number = reader.read_u16().map_err(codec)?;
        let observation_digest = read_digest(&mut reader)?;
        let terminal = decode_outcome(&mut reader)?;
        let late_after_cancellation = reader.read_bool().map_err(codec)?;
        let record = Self {
            rollout_id,
            campaign_id,
            profile_digest,
            task_id,
            arm,
            ordinal,
            seed,
            work_id,
            request_digest,
            attempt: RolloutAttempt::new(
                number,
                observation_digest,
                terminal,
                late_after_cancellation,
            )
            .map_err(|_| corrupt("rollout record attempt number is zero"))?,
            candidate_output: decode_artifact(&mut reader)?,
            candidate_resources: decode_resource(&mut reader)?,
            evaluator_resources: decode_resource(&mut reader)?,
            trace_root: decode_artifact(&mut reader)?,
            evidence_root: decode_artifact(&mut reader)?,
            digest: read_digest(&mut reader)?,
        };
        reader.finish().map_err(codec)?;
        record.validate_recovered()?;
        if record.digest != peritus_codec::sha256(&record.identity_bytes())
            || record.canonical_bytes() != bytes
        {
            return Err(corrupt("rollout record digest or canonical bytes differ"));
        }
        Ok(record)
    }

    /// Checks a recovered record against the complete frozen rollout specification.
    ///
    /// # Errors
    /// Rejects any campaign, profile, task, arm, ordinal, seed, work, or request binding drift.
    pub fn validate_spec(self, spec: &RolloutSpec) -> Result<(), EvaluationError> {
        if self.rollout_id != spec.id()
            || self.campaign_id != spec.campaign_id()
            || self.profile_digest != spec.profile_digest()
            || self.task_id != spec.task_id()
            || self.arm != spec.arm()
            || self.ordinal != spec.ordinal()
            || self.seed != spec.seed()
            || self.work_id != spec.work_id()
            || self.request_digest != spec.request_digest()
        {
            return Err(invalid("terminal record differs from the frozen rollout specification"));
        }
        Ok(())
    }

    /// Logical rollout identity.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Frozen profile digest.
    #[must_use]
    pub const fn profile_digest(self) -> ProfileDigest {
        self.profile_digest
    }
    /// Task identity.
    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
    /// Evaluation arm.
    #[must_use]
    pub const fn arm(self) -> EvaluationArm {
        self.arm
    }
    /// Paired ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u16 {
        self.ordinal
    }
    /// Paired seed.
    #[must_use]
    pub const fn seed(self) -> RolloutSeed {
        self.seed
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
    /// Retained terminal attempt.
    #[must_use]
    pub const fn attempt(self) -> RolloutAttempt {
        self.attempt
    }
    /// Logical terminal outcome.
    #[must_use]
    pub const fn outcome(self) -> RolloutOutcome {
        self.attempt.terminal()
    }
    /// Finalized candidate output when produced.
    #[must_use]
    pub const fn candidate_output(self) -> Option<ArtifactDigest> {
        self.candidate_output
    }
    /// Candidate-stage resources when execution started.
    #[must_use]
    pub const fn candidate_resources(self) -> Option<ResourceObservation> {
        self.candidate_resources
    }
    /// Evaluator-stage resources when it completed.
    #[must_use]
    pub const fn evaluator_resources(self) -> Option<ResourceObservation> {
        self.evaluator_resources
    }
    /// Complete trace root when available.
    #[must_use]
    pub const fn trace_root(self) -> Option<ArtifactDigest> {
        self.trace_root
    }
    /// Per-rollout evidence root when available.
    #[must_use]
    pub const fn evidence_root(self) -> Option<ArtifactDigest> {
        self.evidence_root
    }
    /// Digest binding every record field.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }

    fn identity_bytes(self) -> Vec<u8> {
        let mut bytes = RECORD_DOMAIN.to_vec();
        bytes.extend_from_slice(self.rollout_id.as_bytes());
        bytes.extend_from_slice(self.campaign_id.as_bytes());
        bytes.extend_from_slice(self.profile_digest.as_bytes());
        bytes.extend_from_slice(self.task_id.as_bytes());
        bytes.push(self.arm.tag());
        bytes.extend_from_slice(&self.ordinal.to_be_bytes());
        bytes.extend_from_slice(self.seed.as_bytes());
        bytes.extend_from_slice(self.work_id.as_bytes());
        bytes.extend_from_slice(self.request_digest.as_bytes());
        bytes.extend_from_slice(&self.attempt.number().to_be_bytes());
        bytes.extend_from_slice(self.attempt.observation_digest().as_bytes());
        encode_outcome(&mut bytes, self.outcome());
        bytes.push(u8::from(self.attempt.late_after_cancellation()));
        encode_artifact(&mut bytes, self.candidate_output);
        encode_resource(&mut bytes, self.candidate_resources);
        encode_resource(&mut bytes, self.evaluator_resources);
        encode_artifact(&mut bytes, self.trace_root);
        encode_artifact(&mut bytes, self.evidence_root);
        bytes
    }

    fn validate_recovered(self) -> Result<(), EvaluationError> {
        let rollout_id = crate::identity::derived_identity(
            b"peritus.evaluation.rollout-id.v1\0",
            &[
                self.campaign_id.as_bytes(),
                self.profile_digest.as_bytes(),
                self.task_id.as_bytes(),
                &[self.arm.tag()],
                &self.ordinal.to_be_bytes(),
            ],
        );
        let work_id = crate::identity::derived_identity(
            b"peritus.evaluation.work-id.v1\0",
            &[self.rollout_id.as_bytes(), self.request_digest.as_bytes()],
        );
        if self.rollout_id.as_bytes() != &rollout_id || self.work_id.as_bytes() != &work_id {
            return Err(corrupt("rollout record derived identities differ"));
        }
        let candidate_stage_is_consistent =
            self.candidate_output.is_some() == self.candidate_resources.is_some();
        let evaluator_requires_candidate =
            self.evaluator_resources.is_none() || self.candidate_output.is_some();
        let evaluator_stage_is_consistent =
            self.outcome().evaluated() == self.evaluator_resources.is_some();
        if !candidate_stage_is_consistent
            || !evaluator_requires_candidate
            || !evaluator_stage_is_consistent
        {
            return Err(corrupt("rollout record terminal stage observations are inconsistent"));
        }
        let terminal_digest = match self.outcome() {
            RolloutOutcome::TaskPassed { evaluator_digest }
            | RolloutOutcome::TaskFailed { evaluator_digest, .. } => Some(evaluator_digest),
            RolloutOutcome::InfrastructureFailed { failure_digest, .. } => Some(failure_digest),
            RolloutOutcome::Ambiguous { observation_digest } => Some(observation_digest),
            RolloutOutcome::Cancelled => None,
        };
        if terminal_digest.is_some_and(|digest| digest != self.attempt.observation_digest()) {
            return Err(corrupt("rollout record attempt differs from its terminal observation"));
        }
        Ok(())
    }
}

fn read_digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, EvaluationError> {
    reader.read_fixed().map(Sha256Digest::new).map_err(codec)
}

fn decode_outcome(reader: &mut CanonicalReader<'_>) -> Result<RolloutOutcome, EvaluationError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(RolloutOutcome::TaskPassed { evaluator_digest: read_digest(reader)? }),
        2 => {
            let class = match reader.read_u8().map_err(codec)? {
                1 => crate::TaskFailureClass::Incorrect,
                2 => crate::TaskFailureClass::Safety,
                3 => crate::TaskFailureClass::Incomplete,
                _ => return Err(corrupt("invalid rollout record task failure class")),
            };
            Ok(RolloutOutcome::TaskFailed { class, evaluator_digest: read_digest(reader)? })
        }
        3 => {
            let class = match reader.read_u8().map_err(codec)? {
                1 => crate::InfrastructureFailureClass::Scheduler,
                2 => crate::InfrastructureFailureClass::Execution,
                3 => crate::InfrastructureFailureClass::Provider,
                4 => crate::InfrastructureFailureClass::Evaluator,
                5 => crate::InfrastructureFailureClass::Publication,
                _ => return Err(corrupt("invalid rollout record infrastructure failure class")),
            };
            Ok(RolloutOutcome::InfrastructureFailed {
                class,
                failure_digest: read_digest(reader)?,
                retryable: reader.read_bool().map_err(codec)?,
            })
        }
        4 => Ok(RolloutOutcome::Cancelled),
        5 => Ok(RolloutOutcome::Ambiguous { observation_digest: read_digest(reader)? }),
        _ => Err(corrupt("invalid rollout record outcome")),
    }
}

fn decode_artifact(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<ArtifactDigest>, EvaluationError> {
    reader
        .read_option_tag()
        .map_err(codec)?
        .then(|| read_digest(reader).map(ArtifactDigest::from_sha256))
        .transpose()
}

fn decode_resource(
    reader: &mut CanonicalReader<'_>,
) -> Result<Option<ResourceObservation>, EvaluationError> {
    if !reader.read_option_tag().map_err(codec)? {
        return Ok(None);
    }
    Ok(Some(ResourceObservation::new(
        reader.read_u64().map_err(codec)?,
        decode_optional_u64(reader)?,
        decode_optional_u64(reader)?,
        decode_optional_u64(reader)?,
        decode_optional_u64(reader)?,
        decode_optional_u64(reader)?,
        reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| reader.read_u32().map_err(codec))
            .transpose()?,
        reader.read_bool().map_err(codec)?,
        reader.read_bool().map_err(codec)?,
    )))
}

fn decode_optional_u64(reader: &mut CanonicalReader<'_>) -> Result<Option<u64>, EvaluationError> {
    reader.read_option_tag().map_err(codec)?.then(|| reader.read_u64().map_err(codec)).transpose()
}

fn encode_outcome(bytes: &mut Vec<u8>, outcome: RolloutOutcome) {
    match outcome {
        RolloutOutcome::TaskPassed { evaluator_digest } => {
            bytes.push(1);
            bytes.extend_from_slice(evaluator_digest.as_bytes());
        }
        RolloutOutcome::TaskFailed { class, evaluator_digest } => {
            bytes.push(2);
            bytes.push(match class {
                crate::TaskFailureClass::Incorrect => 1,
                crate::TaskFailureClass::Safety => 2,
                crate::TaskFailureClass::Incomplete => 3,
            });
            bytes.extend_from_slice(evaluator_digest.as_bytes());
        }
        RolloutOutcome::InfrastructureFailed { class, failure_digest, retryable } => {
            bytes.push(3);
            bytes.push(match class {
                crate::InfrastructureFailureClass::Scheduler => 1,
                crate::InfrastructureFailureClass::Execution => 2,
                crate::InfrastructureFailureClass::Provider => 3,
                crate::InfrastructureFailureClass::Evaluator => 4,
                crate::InfrastructureFailureClass::Publication => 5,
            });
            bytes.extend_from_slice(failure_digest.as_bytes());
            bytes.push(u8::from(retryable));
        }
        RolloutOutcome::Cancelled => bytes.push(4),
        RolloutOutcome::Ambiguous { observation_digest } => {
            bytes.push(5);
            bytes.extend_from_slice(observation_digest.as_bytes());
        }
    }
}

fn encode_artifact(bytes: &mut Vec<u8>, value: Option<ArtifactDigest>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(value.as_bytes());
    }
}

fn encode_resource(bytes: &mut Vec<u8>, value: Option<ResourceObservation>) {
    bytes.push(u8::from(value.is_some()));
    if let Some(value) = value {
        bytes.extend_from_slice(&value.elapsed_micros().to_be_bytes());
        for item in [
            value.input_tokens(),
            value.output_tokens(),
            value.cost_microunits(),
            value.memory_high_water_bytes(),
            value.cpu_micros(),
        ] {
            bytes.push(u8::from(item.is_some()));
            if let Some(item) = item {
                bytes.extend_from_slice(&item.to_be_bytes());
            }
        }
        bytes.push(u8::from(value.process_high_water().is_some()));
        if let Some(item) = value.process_high_water() {
            bytes.extend_from_slice(&item.to_be_bytes());
        }
        bytes.push(u8::from(value.trace_complete()));
        bytes.push(u8::from(value.teardown_complete()));
    }
}

const fn invalid(detail: &'static str) -> EvaluationError {
    crate::invalid(EvaluationErrorKind::Binding, EvaluationOperation::Account, detail)
}

const fn corrupt(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Recover,
        crate::EvaluationRecovery::Quarantine,
        detail,
    )
}

const fn codec(_error: peritus_codec::CodecError) -> EvaluationError {
    corrupt("rollout record canonical bytes are malformed")
}
