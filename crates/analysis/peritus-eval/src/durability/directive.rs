//! Stable C0 schedule, execution, cancellation, and publication directives.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_journal::{OutboxId, OutboxMessage, OutboxState};
use peritus_scheduler::{WorkId, WorkSpec};

use crate::{
    EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    EvaluationRecovery, ReportRecord, RolloutId,
};

/// Destination for exact D3 submit/cancel effects.
pub const SCHEDULE_DESTINATION: &str = "peritus.eval.schedule-rollout.v1";
/// Destination for exact candidate/evaluator execute/cancel effects.
pub const EXECUTION_DESTINATION: &str = "peritus.eval.execute-rollout.v1";
/// Destination for report artifact/evidence publication.
pub const PUBLICATION_DESTINATION: &str = "peritus.eval.publish-report.v1";

const SCHEDULE_DOMAIN: &[u8] = b"peritus.evaluation.schedule-directive.v1\0";
const EXECUTION_DOMAIN: &[u8] = b"peritus.evaluation.execution-directive.v1\0";
const PUBLICATION_DOMAIN: &[u8] = b"peritus.evaluation.publication-directive.v1\0";
const SCHEDULE_ID_DOMAIN: &[u8] = b"peritus.evaluation.schedule-outbox.v1\0";
const EXECUTION_ID_DOMAIN: &[u8] = b"peritus.evaluation.execution-outbox.v1\0";
const PUBLICATION_ID_DOMAIN: &[u8] = b"peritus.evaluation.publication-outbox.v1\0";

/// D3 schedule effect kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleDirectiveKind {
    /// Submit this exact inert D3 work specification.
    Submit(Box<WorkSpec>),
    /// Cancel this exact D3 work identity.
    Cancel(WorkId),
}

/// Complete D3 schedule/cancel directive for one rollout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleDirective {
    campaign_id: EvaluationCampaignId,
    rollout_id: RolloutId,
    kind: ScheduleDirectiveKind,
}

impl ScheduleDirective {
    /// Creates one exact D3 submit directive.
    ///
    /// # Errors
    /// Rejects a work/payload identity that differs from the planned binding.
    pub fn submit(
        campaign_id: EvaluationCampaignId,
        rollout_id: RolloutId,
        work: WorkSpec,
    ) -> Result<Self, EvaluationError> {
        if work.class() != peritus_scheduler::ExecutionClass::Coordination {
            return Err(binding("schedule directive is not coordination work"));
        }
        Ok(Self { campaign_id, rollout_id, kind: ScheduleDirectiveKind::Submit(Box::new(work)) })
    }
    /// Creates one exact D3 cancellation directive.
    #[must_use]
    pub const fn cancel(
        campaign_id: EvaluationCampaignId,
        rollout_id: RolloutId,
        work_id: WorkId,
    ) -> Self {
        Self { campaign_id, rollout_id, kind: ScheduleDirectiveKind::Cancel(work_id) }
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Logical rollout.
    #[must_use]
    pub const fn rollout_id(&self) -> RolloutId {
        self.rollout_id
    }
    /// Exact schedule effect.
    #[must_use]
    pub const fn kind(&self) -> &ScheduleDirectiveKind {
        &self.kind
    }

    pub(crate) fn canonical_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(SCHEDULE_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.campaign_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.rollout_id.as_bytes()).map_err(codec)?;
        match &self.kind {
            ScheduleDirectiveKind::Submit(work) => {
                writer.write_u8(1).map_err(codec)?;
                crate::encode_evaluation_work(&mut writer, work)?;
            }
            ScheduleDirectiveKind::Cancel(work) => {
                writer.write_u8(2).map_err(codec)?;
                writer.write_fixed(work.as_bytes()).map_err(codec)?;
            }
        }
        Ok(writer.into_bytes())
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != SCHEDULE_DOMAIN {
            return Err(corrupt("unsupported schedule directive domain"));
        }
        let campaign_id = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid schedule campaign identity"))?;
        let rollout_id = RolloutId::new(reader.read_fixed().map_err(codec)?)?;
        let kind = match reader.read_u8().map_err(codec)? {
            1 => ScheduleDirectiveKind::Submit(Box::new(crate::wire::decode_work(&mut reader)?)),
            2 => ScheduleDirectiveKind::Cancel(
                WorkId::new(reader.read_fixed().map_err(codec)?)
                    .map_err(|_| corrupt("invalid schedule work identity"))?,
            ),
            _ => return Err(corrupt("unknown schedule directive kind")),
        };
        reader.finish().map_err(codec)?;
        Ok(Self { campaign_id, rollout_id, kind })
    }
    pub(crate) fn outbox_id(&self) -> Result<OutboxId, EvaluationError> {
        let mut semantic = Vec::new();
        match &self.kind {
            ScheduleDirectiveKind::Submit(work) => {
                semantic.push(1);
                semantic.extend_from_slice(work.id().as_bytes());
                semantic.extend_from_slice(work.payload_digest().as_bytes());
            }
            ScheduleDirectiveKind::Cancel(work) => {
                semantic.push(2);
                semantic.extend_from_slice(work.as_bytes());
            }
        }
        derived_outbox_id(
            SCHEDULE_ID_DOMAIN,
            self.campaign_id,
            self.rollout_id.as_bytes(),
            &semantic,
        )
    }
}

/// Candidate/evaluator execution effect kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionDirectiveKind {
    /// Execute the exact frozen request.
    Execute {
        /// Complete frozen execution request digest.
        request_digest: peritus_types::Sha256Digest,
    },
    /// Cancel owned external execution for the rollout.
    Cancel,
}

/// Exact runtime-neutral execute/cancel directive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionDirective {
    campaign_id: EvaluationCampaignId,
    rollout_id: RolloutId,
    kind: ExecutionDirectiveKind,
}

impl ExecutionDirective {
    /// Creates one exact execution directive.
    #[must_use]
    pub const fn execute(
        campaign_id: EvaluationCampaignId,
        rollout_id: RolloutId,
        request_digest: peritus_types::Sha256Digest,
    ) -> Self {
        Self { campaign_id, rollout_id, kind: ExecutionDirectiveKind::Execute { request_digest } }
    }
    /// Creates one exact cancellation directive.
    #[must_use]
    pub const fn cancel(campaign_id: EvaluationCampaignId, rollout_id: RolloutId) -> Self {
        Self { campaign_id, rollout_id, kind: ExecutionDirectiveKind::Cancel }
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Logical rollout.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Exact execution effect.
    #[must_use]
    pub const fn kind(self) -> ExecutionDirectiveKind {
        self.kind
    }
    pub(crate) fn canonical_bytes(self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(EXECUTION_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.campaign_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.rollout_id.as_bytes()).map_err(codec)?;
        match self.kind {
            ExecutionDirectiveKind::Execute { request_digest } => {
                writer.write_u8(1).map_err(codec)?;
                writer.write_fixed(request_digest.as_bytes()).map_err(codec)?;
            }
            ExecutionDirectiveKind::Cancel => writer.write_u8(2).map_err(codec)?,
        }
        Ok(writer.into_bytes())
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != EXECUTION_DOMAIN {
            return Err(corrupt("unsupported execution directive domain"));
        }
        let campaign_id = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid execution campaign identity"))?;
        let rollout_id = RolloutId::new(reader.read_fixed().map_err(codec)?)?;
        let kind = match reader.read_u8().map_err(codec)? {
            1 => ExecutionDirectiveKind::Execute {
                request_digest: peritus_types::Sha256Digest::new(
                    reader.read_fixed().map_err(codec)?,
                ),
            },
            2 => ExecutionDirectiveKind::Cancel,
            _ => return Err(corrupt("unknown execution directive kind")),
        };
        reader.finish().map_err(codec)?;
        Ok(Self { campaign_id, rollout_id, kind })
    }
    pub(crate) fn outbox_id(self) -> Result<OutboxId, EvaluationError> {
        let mut semantic = vec![match self.kind {
            ExecutionDirectiveKind::Execute { .. } => 1,
            ExecutionDirectiveKind::Cancel => 2,
        }];
        if let ExecutionDirectiveKind::Execute { request_digest } = self.kind {
            semantic.extend_from_slice(request_digest.as_bytes());
        }
        derived_outbox_id(
            EXECUTION_ID_DOMAIN,
            self.campaign_id,
            self.rollout_id.as_bytes(),
            &semantic,
        )
    }
}

/// Exact publication directive for one committed report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationDirective {
    campaign_id: EvaluationCampaignId,
    report: ReportRecord,
}

impl PublicationDirective {
    /// Creates one exact report publication directive.
    #[must_use]
    pub const fn new(campaign_id: EvaluationCampaignId, report: ReportRecord) -> Self {
        Self { campaign_id, report }
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Committed report record.
    #[must_use]
    pub const fn report(self) -> ReportRecord {
        self.report
    }
    pub(crate) fn canonical_bytes(self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_bytes(PUBLICATION_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.campaign_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.report.id().as_bytes()).map_err(codec)?;
        writer.write_fixed(self.report.payload_digest().as_bytes()).map_err(codec)?;
        writer.write_fixed(self.report.artifact().as_bytes()).map_err(codec)?;
        writer.write_u64(self.report.size()).map_err(codec)?;
        Ok(writer.into_bytes())
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, EvaluationError> {
        let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
        if reader.read_bytes().map_err(codec)? != PUBLICATION_DOMAIN {
            return Err(corrupt("unsupported publication directive domain"));
        }
        let campaign_id = EvaluationCampaignId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| corrupt("invalid publication campaign identity"))?;
        let report = ReportRecord::new(
            crate::EvaluationReportId::new(reader.read_fixed().map_err(codec)?)?,
            peritus_types::Sha256Digest::new(reader.read_fixed().map_err(codec)?),
            peritus_artifact_store::ArtifactDigest::from_sha256(peritus_types::Sha256Digest::new(
                reader.read_fixed().map_err(codec)?,
            )),
            reader.read_u64().map_err(codec)?,
        )?;
        reader.finish().map_err(codec)?;
        Ok(Self { campaign_id, report })
    }
    pub(crate) fn outbox_id(self) -> Result<OutboxId, EvaluationError> {
        derived_outbox_id(
            PUBLICATION_ID_DOMAIN,
            self.campaign_id,
            self.report.id().as_bytes(),
            self.report.artifact().as_bytes(),
        )
    }
}

/// An exact claimed C0 schedule directive with its positive fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleDirectiveClaim {
    directive: ScheduleDirective,
    fence: u64,
}

impl ScheduleDirectiveClaim {
    /// Validates destination, payload, identity, claimed state, and fence.
    ///
    /// # Errors
    /// Rejects an unclaimed, misrouted, malformed, or identity-inconsistent message.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, EvaluationError> {
        ensure_claimed(message, SCHEDULE_DESTINATION)?;
        let directive = ScheduleDirective::decode(message.payload())?;
        if message.id() != directive.outbox_id()? {
            return Err(binding("outbox identity differs from canonical payload"));
        }
        Ok(Self {
            directive,
            fence: message.fence().ok_or_else(|| binding("claimed directive has no fence"))?,
        })
    }

    /// Exact decoded directive.
    #[must_use]
    pub const fn directive(&self) -> &ScheduleDirective {
        &self.directive
    }

    /// Exact positive claim fence.
    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
    }
}

/// An exact claimed C0 execution directive with its positive fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionDirectiveClaim {
    directive: ExecutionDirective,
    fence: u64,
}

impl ExecutionDirectiveClaim {
    /// Validates destination, payload, identity, claimed state, and fence.
    ///
    /// # Errors
    /// Rejects an unclaimed, misrouted, malformed, or identity-inconsistent message.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, EvaluationError> {
        ensure_claimed(message, EXECUTION_DESTINATION)?;
        let directive = ExecutionDirective::decode(message.payload())?;
        if message.id() != directive.outbox_id()? {
            return Err(binding("outbox identity differs from canonical payload"));
        }
        Ok(Self {
            directive,
            fence: message.fence().ok_or_else(|| binding("claimed directive has no fence"))?,
        })
    }

    /// Exact decoded directive.
    #[must_use]
    pub const fn directive(&self) -> &ExecutionDirective {
        &self.directive
    }

    /// Exact positive claim fence.
    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
    }
}

/// An exact claimed C0 publication directive with its positive fence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationDirectiveClaim {
    directive: PublicationDirective,
    fence: u64,
}

impl PublicationDirectiveClaim {
    /// Validates destination, payload, identity, claimed state, and fence.
    ///
    /// # Errors
    /// Rejects an unclaimed, misrouted, malformed, or identity-inconsistent message.
    pub fn from_message(message: &OutboxMessage) -> Result<Self, EvaluationError> {
        ensure_claimed(message, PUBLICATION_DESTINATION)?;
        let directive = PublicationDirective::decode(message.payload())?;
        if message.id() != directive.outbox_id()? {
            return Err(binding("outbox identity differs from canonical payload"));
        }
        Ok(Self {
            directive,
            fence: message.fence().ok_or_else(|| binding("claimed directive has no fence"))?,
        })
    }

    /// Exact decoded directive.
    #[must_use]
    pub const fn directive(&self) -> &PublicationDirective {
        &self.directive
    }

    /// Exact positive claim fence.
    #[must_use]
    pub const fn fence(&self) -> u64 {
        self.fence
    }
}

/// Any exact E3 effect-lane claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationDirectiveClaim {
    /// D3 schedule/cancel claim.
    Schedule(ScheduleDirectiveClaim),
    /// Candidate/evaluator execute/cancel claim.
    Execution(ExecutionDirectiveClaim),
    /// Report publication claim.
    Publication(PublicationDirectiveClaim),
}

impl EvaluationDirectiveClaim {
    pub(crate) fn id(&self) -> Result<OutboxId, EvaluationError> {
        match self {
            Self::Schedule(value) => value.directive.outbox_id(),
            Self::Execution(value) => value.directive.outbox_id(),
            Self::Publication(value) => value.directive.outbox_id(),
        }
    }
    pub(crate) const fn fence(&self) -> u64 {
        match self {
            Self::Schedule(value) => value.fence,
            Self::Execution(value) => value.fence,
            Self::Publication(value) => value.fence,
        }
    }
}

impl From<ScheduleDirectiveClaim> for EvaluationDirectiveClaim {
    fn from(value: ScheduleDirectiveClaim) -> Self {
        Self::Schedule(value)
    }
}
impl From<ExecutionDirectiveClaim> for EvaluationDirectiveClaim {
    fn from(value: ExecutionDirectiveClaim) -> Self {
        Self::Execution(value)
    }
}
impl From<PublicationDirectiveClaim> for EvaluationDirectiveClaim {
    fn from(value: PublicationDirectiveClaim) -> Self {
        Self::Publication(value)
    }
}

fn ensure_claimed(message: &OutboxMessage, destination: &str) -> Result<(), EvaluationError> {
    if message.state() != OutboxState::Claimed
        || message.destination() != destination
        || message.fence().is_none_or(|value| value == 0)
    {
        return Err(binding("outbox message is not an exact claimed evaluation directive"));
    }
    Ok(())
}

fn derived_outbox_id(
    domain: &[u8],
    campaign_id: EvaluationCampaignId,
    identity: &[u8],
    semantic: &[u8],
) -> Result<OutboxId, EvaluationError> {
    let mut bytes = Vec::with_capacity(domain.len() + 16 + identity.len() + semantic.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(campaign_id.as_bytes());
    bytes.extend_from_slice(identity);
    bytes.extend_from_slice(semantic);
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 0x40;
    OutboxId::new(id).map_err(|_| binding("derived outbox identity is invalid"))
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation directive violates canonical codec bounds",
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
const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Commit,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
