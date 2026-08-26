//! Complete compact family-87 evaluation checkpoint.

use std::collections::BTreeMap;

use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_scheduler::WorkId;
use peritus_types::{
    AcceptanceSpecId, EventId, EvidenceId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    CampaignFailure, CampaignFailureCode, DatasetDigest, EvaluationCampaignId, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationPhase, EvaluationPlanId,
    EvaluationRecovery, EvaluationReportId, LedgerCounts, PlanDigest, PlanRecord,
    PlannedRolloutBinding, ProfileDigest, PublicationRecord, ReportRecord, ResultDigest, RolloutId,
    RolloutProgress, RolloutStatus, RolloutTerminalClass, TerminalRecordRef,
};

const STATE_DOMAIN: &[u8] = b"peritus.evaluation.state.v1\0";

/// Complete authoritative E3 campaign state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationState {
    pub(crate) campaign_id: EvaluationCampaignId,
    pub(crate) revision: RevisionTuple,
    pub(crate) dataset_digest: DatasetDigest,
    pub(crate) dataset_artifact: ArtifactDigest,
    pub(crate) profile_artifact: ArtifactDigest,
    pub(crate) profile_digest: ProfileDigest,
    pub(crate) sequence: u64,
    pub(crate) last_event_id: EventId,
    pub(crate) state_digest: Sha256Digest,
    pub(crate) phase: EvaluationPhase,
    pub(crate) batch_total: Option<u32>,
    pub(crate) pending_plan_id: Option<EvaluationPlanId>,
    pub(crate) pending_plan_digest: Option<PlanDigest>,
    pub(crate) batch_artifacts: Vec<ArtifactDigest>,
    pub(crate) rollouts: BTreeMap<RolloutId, RolloutProgress>,
    pub(crate) plan: Option<PlanRecord>,
    pub(crate) analysis_digest: Option<ResultDigest>,
    pub(crate) analysis_artifact: Option<ArtifactDigest>,
    pub(crate) analysis_artifact_bytes: Option<u64>,
    pub(crate) analysis_counts: Option<LedgerCounts>,
    pub(crate) report: Option<ReportRecord>,
    pub(crate) publication: Option<PublicationRecord>,
    pub(crate) cancellation_reason: Option<Sha256Digest>,
    pub(crate) failure: Option<CampaignFailure>,
}

impl EvaluationState {
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Cross-slice provenance revision.
    #[must_use]
    pub const fn revision(&self) -> &RevisionTuple {
        &self.revision
    }
    /// Frozen dataset digest.
    #[must_use]
    pub const fn dataset_digest(&self) -> DatasetDigest {
        self.dataset_digest
    }
    /// Finalized canonical dataset manifest artifact.
    #[must_use]
    pub const fn dataset_artifact(&self) -> ArtifactDigest {
        self.dataset_artifact
    }
    /// Finalized canonical frozen-profile artifact.
    #[must_use]
    pub const fn profile_artifact(&self) -> ArtifactDigest {
        self.profile_artifact
    }
    /// Frozen evaluation profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Applied event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Aggregate head event.
    #[must_use]
    pub const fn last_event_id(&self) -> EventId {
        self.last_event_id
    }
    /// Digest of every complete state field.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
    /// Durable campaign phase.
    #[must_use]
    pub const fn phase(&self) -> EvaluationPhase {
        self.phase
    }
    /// Complete plan root when finalized.
    #[must_use]
    pub const fn plan(&self) -> Option<PlanRecord> {
        self.plan
    }
    /// Returns progress for one planned rollout.
    #[must_use]
    pub fn rollout(&self, id: RolloutId) -> Option<RolloutProgress> {
        self.rollouts.get(&id).copied()
    }
    /// Iterates rollout progress in canonical identity order.
    #[must_use]
    pub fn rollouts(&self) -> std::vec::IntoIter<(RolloutId, RolloutProgress)> {
        self.rollouts.iter().map(|(id, progress)| (*id, *progress)).collect::<Vec<_>>().into_iter()
    }
    /// Complete deterministic analysis digest when committed.
    #[must_use]
    pub const fn analysis_digest(&self) -> Option<ResultDigest> {
        self.analysis_digest
    }
    /// Finalized deterministic analysis artifact.
    #[must_use]
    pub const fn analysis_artifact(&self) -> Option<ArtifactDigest> {
        self.analysis_artifact
    }
    /// Exact analysis artifact size.
    #[must_use]
    pub const fn analysis_artifact_bytes(&self) -> Option<u64> {
        self.analysis_artifact_bytes
    }
    /// Conserved ledger counts admitted at analysis start.
    #[must_use]
    pub const fn analysis_counts(&self) -> Option<LedgerCounts> {
        self.analysis_counts
    }
    /// Canonical report artifact when ready.
    #[must_use]
    pub const fn report(&self) -> Option<ReportRecord> {
        self.report
    }
    /// Evidence-backed publication when terminal.
    #[must_use]
    pub const fn publication(&self) -> Option<PublicationRecord> {
        self.publication
    }
    /// Durable cancellation reason digest.
    #[must_use]
    pub const fn cancellation_reason(&self) -> Option<Sha256Digest> {
        self.cancellation_reason
    }
    /// Terminal typed failure.
    #[must_use]
    pub const fn failure(&self) -> Option<CampaignFailure> {
        self.failure
    }

    /// Computes raw terminal counts from the complete progress map.
    #[must_use]
    pub fn counts(&self) -> LedgerCounts {
        let mut counts = LedgerCounts {
            expected: u32::try_from(self.rollouts.len()).unwrap_or(u32::MAX),
            ..LedgerCounts::default()
        };
        for progress in self.rollouts.values() {
            match progress.status() {
                RolloutStatus::Settled(record) => match record.class() {
                    RolloutTerminalClass::Passed => counts.passed += 1,
                    RolloutTerminalClass::TaskFailed => counts.task_failed += 1,
                    RolloutTerminalClass::InfrastructureFailed => counts.infrastructure_failed += 1,
                    RolloutTerminalClass::Ambiguous => counts.ambiguous += 1,
                },
                RolloutStatus::Cancelled { .. } => counts.cancelled += 1,
                _ => {}
            }
        }
        counts
    }

    /// Canonically encodes the complete state and advertised digest.
    ///
    /// # Errors
    /// Returns a codec error when the bounded checkpoint exceeds production limits.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        encode_identity(&mut writer, self)?;
        writer.write_fixed(self.state_digest.as_bytes()).map_err(codec)?;
        Ok(writer.into_bytes())
    }

    pub(crate) fn refresh_digest(&mut self) -> Result<(), EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        encode_identity(&mut writer, self)?;
        self.state_digest = peritus_codec::sha256(&writer.into_bytes());
        Ok(())
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the strict family-87 decoder validates one closed field order without hidden partial state"
    )]
    pub(crate) fn decode_canonical(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != STATE_DOMAIN {
            return Err(corrupt("unsupported evaluation state domain"));
        }
        let campaign_id = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid campaign identity"))?;
        let revision = decode_revision(&mut reader)?;
        let dataset_digest = DatasetDigest::new(digest(&mut reader)?);
        let dataset_artifact = ArtifactDigest::from_sha256(digest(&mut reader)?);
        let profile_artifact = ArtifactDigest::from_sha256(digest(&mut reader)?);
        let profile_digest = ProfileDigest::new(digest(&mut reader)?);
        let sequence = reader.read_u64().map_err(codec)?;
        let last_event_id = EventId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid state event identity"))?;
        let phase = EvaluationPhase::from_tag(reader.read_u8().map_err(codec)?)?;
        let batch_total = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| reader.read_u32().map_err(codec))
            .transpose()?;
        let pending_plan_id = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| EvaluationPlanId::new(reader.read_fixed().map_err(codec)?))
            .transpose()?;
        let pending_plan_digest = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| digest(&mut reader).map(PlanDigest::new))
            .transpose()?;
        if pending_plan_id.is_some() != pending_plan_digest.is_some() {
            return Err(corrupt("pending plan identity and digest presence differ"));
        }
        let batch_len = reader.read_collection_len().map_err(codec)?;
        let mut batch_artifacts = Vec::with_capacity(batch_len);
        for _ in 0..batch_len {
            batch_artifacts.push(ArtifactDigest::from_sha256(digest(&mut reader)?));
        }
        let rollout_len = reader.read_collection_len().map_err(codec)?;
        let mut rollouts = BTreeMap::new();
        for _ in 0..rollout_len {
            let id = RolloutId::new(reader.read_fixed().map_err(codec)?)?;
            let binding = PlannedRolloutBinding::new(
                id,
                WorkId::new(reader.read_fixed().map_err(codec)?)
                    .map_err(|_| corrupt("invalid work identity"))?,
                digest(&mut reader)?,
            );
            let attempts = reader.read_u16().map_err(codec)?;
            let status = decode_status(&mut reader)?;
            let progress = RolloutProgress::decoded(binding, status, attempts);
            if rollouts.insert(id, progress).is_some() {
                return Err(corrupt("duplicate rollout in evaluation checkpoint"));
            }
        }
        let plan = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| decode_plan(&mut reader))
            .transpose()?;
        let analysis_digest = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| digest(&mut reader).map(ResultDigest::new))
            .transpose()?;
        let analysis_artifact = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| digest(&mut reader).map(ArtifactDigest::from_sha256))
            .transpose()?;
        let analysis_artifact_bytes = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| reader.read_u64().map_err(codec))
            .transpose()?;
        if analysis_artifact.is_some() != analysis_artifact_bytes.is_some() {
            return Err(corrupt("analysis artifact identity and size presence differ"));
        }
        let analysis_counts = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| decode_counts(&mut reader))
            .transpose()?;
        let report = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| decode_report(&mut reader))
            .transpose()?;
        let publication = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| decode_publication(&mut reader))
            .transpose()?;
        let cancellation_reason =
            reader.read_option_tag().map_err(codec)?.then(|| digest(&mut reader)).transpose()?;
        let failure = reader
            .read_option_tag()
            .map_err(codec)?
            .then(|| {
                Ok::<_, EvaluationError>(CampaignFailure::new(
                    CampaignFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
                    digest(&mut reader)?,
                ))
            })
            .transpose()?;
        let state_digest = digest(&mut reader)?;
        reader.finish().map_err(codec)?;
        let mut state = Self {
            campaign_id,
            revision,
            dataset_digest,
            dataset_artifact,
            profile_artifact,
            profile_digest,
            sequence,
            last_event_id,
            state_digest,
            phase,
            batch_total,
            pending_plan_id,
            pending_plan_digest,
            batch_artifacts,
            rollouts,
            plan,
            analysis_digest,
            analysis_artifact,
            analysis_artifact_bytes,
            analysis_counts,
            report,
            publication,
            cancellation_reason,
            failure,
        };
        let advertised = state.state_digest;
        state.refresh_digest()?;
        if state.state_digest != advertised {
            return Err(corrupt("evaluation state digest disagrees with complete fields"));
        }
        Ok(state)
    }
}

fn encode_identity(
    writer: &mut CanonicalWriter,
    state: &EvaluationState,
) -> Result<(), EvaluationError> {
    writer.write_bytes(STATE_DOMAIN).map_err(codec)?;
    writer.write_fixed(state.campaign_id.as_bytes()).map_err(codec)?;
    encode_revision(writer, state.revision)?;
    writer.write_fixed(state.dataset_digest.as_bytes()).map_err(codec)?;
    writer.write_fixed(state.dataset_artifact.as_bytes()).map_err(codec)?;
    writer.write_fixed(state.profile_artifact.as_bytes()).map_err(codec)?;
    writer.write_fixed(state.profile_digest.as_bytes()).map_err(codec)?;
    writer.write_u64(state.sequence).map_err(codec)?;
    writer.write_fixed(state.last_event_id.as_bytes()).map_err(codec)?;
    writer.write_u8(state.phase.tag()).map_err(codec)?;
    option_u32(writer, state.batch_total)?;
    writer.write_option_tag(state.pending_plan_id.is_some()).map_err(codec)?;
    if let Some(value) = state.pending_plan_id {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_option_tag(state.pending_plan_digest.is_some()).map_err(codec)?;
    if let Some(value) = state.pending_plan_digest {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_collection_len(state.batch_artifacts.len()).map_err(codec)?;
    for artifact in &state.batch_artifacts {
        writer.write_fixed(artifact.as_bytes()).map_err(codec)?;
    }
    writer.write_collection_len(state.rollouts.len()).map_err(codec)?;
    for (id, progress) in &state.rollouts {
        writer.write_fixed(id.as_bytes()).map_err(codec)?;
        writer.write_fixed(progress.binding().work_id().as_bytes()).map_err(codec)?;
        writer.write_fixed(progress.binding().request_digest().as_bytes()).map_err(codec)?;
        writer.write_u16(progress.attempts_retained()).map_err(codec)?;
        encode_status(writer, progress.status())?;
    }
    writer.write_option_tag(state.plan.is_some()).map_err(codec)?;
    if let Some(value) = state.plan {
        encode_plan(writer, value)?;
    }
    writer.write_option_tag(state.analysis_digest.is_some()).map_err(codec)?;
    if let Some(value) = state.analysis_digest {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_option_tag(state.analysis_artifact.is_some()).map_err(codec)?;
    if let Some(value) = state.analysis_artifact {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_option_tag(state.analysis_artifact_bytes.is_some()).map_err(codec)?;
    if let Some(value) = state.analysis_artifact_bytes {
        writer.write_u64(value).map_err(codec)?;
    }
    writer.write_option_tag(state.analysis_counts.is_some()).map_err(codec)?;
    if let Some(value) = state.analysis_counts {
        encode_counts(writer, value)?;
    }
    writer.write_option_tag(state.report.is_some()).map_err(codec)?;
    if let Some(value) = state.report {
        encode_report(writer, value)?;
    }
    writer.write_option_tag(state.publication.is_some()).map_err(codec)?;
    if let Some(value) = state.publication {
        encode_publication(writer, value)?;
    }
    writer.write_option_tag(state.cancellation_reason.is_some()).map_err(codec)?;
    if let Some(value) = state.cancellation_reason {
        writer.write_fixed(value.as_bytes()).map_err(codec)?;
    }
    writer.write_option_tag(state.failure.is_some()).map_err(codec)?;
    if let Some(value) = state.failure {
        writer.write_u8(value.code().tag()).map_err(codec)?;
        writer.write_fixed(value.digest().as_bytes()).map_err(codec)?;
    }
    Ok(())
}

fn encode_status(
    writer: &mut CanonicalWriter,
    value: RolloutStatus,
) -> Result<(), EvaluationError> {
    match value {
        RolloutStatus::Planned => writer.write_u8(1).map_err(codec)?,
        RolloutStatus::Scheduling => writer.write_u8(2).map_err(codec)?,
        RolloutStatus::Scheduled { acknowledgement_digest } => {
            writer.write_u8(3).map_err(codec)?;
            writer.write_fixed(acknowledgement_digest.as_bytes()).map_err(codec)?;
        }
        RolloutStatus::Running { attempt } => {
            writer.write_u8(4).map_err(codec)?;
            writer.write_u16(attempt).map_err(codec)?;
        }
        RolloutStatus::Settled(record) => {
            writer.write_u8(5).map_err(codec)?;
            writer.write_u8(record.class().tag()).map_err(codec)?;
            writer.write_fixed(record.record_digest().as_bytes()).map_err(codec)?;
            writer.write_fixed(record.artifact().as_bytes()).map_err(codec)?;
            writer.write_u64(record.artifact_bytes()).map_err(codec)?;
            writer.write_u16(record.attempt()).map_err(codec)?;
        }
        RolloutStatus::Cancelled { reason_digest, observation_digest } => {
            writer.write_u8(6).map_err(codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(codec)?;
            writer.write_fixed(observation_digest.as_bytes()).map_err(codec)?;
        }
    }
    Ok(())
}

fn decode_status(reader: &mut CanonicalReader<'_>) -> Result<RolloutStatus, EvaluationError> {
    match reader.read_u8().map_err(codec)? {
        1 => Ok(RolloutStatus::Planned),
        2 => Ok(RolloutStatus::Scheduling),
        3 => Ok(RolloutStatus::Scheduled { acknowledgement_digest: digest(reader)? }),
        4 => {
            let attempt = reader.read_u16().map_err(codec)?;
            if attempt == 0 {
                Err(corrupt("running rollout attempt is zero"))
            } else {
                Ok(RolloutStatus::Running { attempt })
            }
        }
        5 => Ok(RolloutStatus::Settled(TerminalRecordRef::new(
            RolloutTerminalClass::from_tag(reader.read_u8().map_err(codec)?)?,
            digest(reader)?,
            ArtifactDigest::from_sha256(digest(reader)?),
            reader.read_u64().map_err(codec)?,
            reader.read_u16().map_err(codec)?,
        )?)),
        6 => Ok(RolloutStatus::Cancelled {
            reason_digest: digest(reader)?,
            observation_digest: digest(reader)?,
        }),
        _ => Err(corrupt("unknown rollout progress tag")),
    }
}

fn encode_plan(writer: &mut CanonicalWriter, value: PlanRecord) -> Result<(), EvaluationError> {
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.root().as_bytes()).map_err(codec)?;
    writer.write_u32(value.expected_rollouts()).map_err(codec)?;
    writer.write_u32(value.total_batches()).map_err(codec)
}
fn decode_plan(reader: &mut CanonicalReader<'_>) -> Result<PlanRecord, EvaluationError> {
    PlanRecord::new(
        EvaluationPlanId::new(reader.read_fixed().map_err(codec)?)?,
        PlanDigest::new(digest(reader)?),
        ArtifactDigest::from_sha256(digest(reader)?),
        reader.read_u32().map_err(codec)?,
        reader.read_u32().map_err(codec)?,
    )
}
fn encode_counts(writer: &mut CanonicalWriter, value: LedgerCounts) -> Result<(), EvaluationError> {
    for item in [
        value.expected,
        value.passed,
        value.task_failed,
        value.infrastructure_failed,
        value.cancelled,
        value.ambiguous,
    ] {
        writer.write_u32(item).map_err(codec)?;
    }
    Ok(())
}
fn decode_counts(reader: &mut CanonicalReader<'_>) -> Result<LedgerCounts, EvaluationError> {
    Ok(LedgerCounts {
        expected: reader.read_u32().map_err(codec)?,
        passed: reader.read_u32().map_err(codec)?,
        task_failed: reader.read_u32().map_err(codec)?,
        infrastructure_failed: reader.read_u32().map_err(codec)?,
        cancelled: reader.read_u32().map_err(codec)?,
        ambiguous: reader.read_u32().map_err(codec)?,
    })
}
fn encode_report(writer: &mut CanonicalWriter, value: ReportRecord) -> Result<(), EvaluationError> {
    writer.write_fixed(value.id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.payload_digest().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.artifact().as_bytes()).map_err(codec)?;
    writer.write_u64(value.size()).map_err(codec)
}
fn decode_report(reader: &mut CanonicalReader<'_>) -> Result<ReportRecord, EvaluationError> {
    ReportRecord::new(
        EvaluationReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        ArtifactDigest::from_sha256(digest(reader)?),
        reader.read_u64().map_err(codec)?,
    )
}
fn encode_publication(
    writer: &mut CanonicalWriter,
    value: PublicationRecord,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.report_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.evidence_id().as_bytes()).map_err(codec)?;
    writer.write_u64(value.report_commit_position()).map_err(codec)
}
fn decode_publication(
    reader: &mut CanonicalReader<'_>,
) -> Result<PublicationRecord, EvaluationError> {
    PublicationRecord::new(
        EvaluationReportId::new(reader.read_fixed().map_err(codec)?)?,
        EvidenceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid publication evidence identity"))?,
        reader.read_u64().map_err(codec)?,
    )
}

fn encode_revision(
    writer: &mut CanonicalWriter,
    value: RevisionTuple,
) -> Result<(), EvaluationError> {
    writer.write_fixed(value.acceptance_spec_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.harness_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.workspace_id().as_bytes()).map_err(codec)?;
    writer.write_u64(value.workspace_generation().get()).map_err(codec)?;
    writer.write_u64(value.workspace_revision().get()).map_err(codec)?;
    writer.write_fixed(value.policy_id().as_bytes()).map_err(codec)?;
    writer.write_fixed(value.provider_profile_id().as_bytes()).map_err(codec)
}
fn decode_revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, EvaluationError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid acceptance identity"))?,
        HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid harness identity"))?,
        WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace identity"))?,
        Generation::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace generation"))?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| corrupt("invalid workspace revision"))?,
        PolicyId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid policy identity"))?,
        ProviderProfileId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid provider identity"))?,
    ))
}

fn option_u32(writer: &mut CanonicalWriter, value: Option<u32>) -> Result<(), EvaluationError> {
    writer.write_option_tag(value.is_some()).map_err(codec)?;
    if let Some(value) = value {
        writer.write_u32(value).map_err(codec)?;
    }
    Ok(())
}
fn digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, EvaluationError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec)?))
}
const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation checkpoint violates canonical codec bounds",
    )
}
const fn corrupt(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
