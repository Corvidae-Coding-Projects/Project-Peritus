//! Strict schema-v1 evaluation command/event semantic codec.

use peritus_artifact_store::ArtifactDigest;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_scheduler::{
    AttemptNumber, ExecutionClass, RecoveryPolicy, ResourceEntry, ResourceKind, ResourceQuantity,
    ResourceVector, SchedulerLimits, WorkId, WorkSpec,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, BudgetReservationId, EvidenceId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    CampaignFailure, CampaignFailureCode, DatasetDigest, EvaluationCommandKind, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationPlanId, EvaluationRecovery,
    EvaluationReportId, LedgerCounts, PlanBatch, PlanDigest, PlanRecord, PlannedRolloutBinding,
    PublicationRecord, ReportRecord, ResultDigest, RolloutId, RolloutTerminalClass,
    TerminalRecordRef,
};

pub(super) fn encode(kind: &EvaluationCommandKind) -> Result<Vec<u8>, EvaluationError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    crate::encode_evaluation_kind(&mut writer, kind)?;
    Ok(writer.into_bytes())
}

pub(super) fn decode(bytes: &[u8]) -> Result<EvaluationCommandKind, EvaluationError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let kind = match reader.read_u8().map_err(codec)? {
        1 => EvaluationCommandKind::CreateCampaign {
            revision: revision(&mut reader)?,
            dataset_digest: DatasetDigest::new(digest(&mut reader)?),
            dataset_artifact: ArtifactDigest::from_sha256(digest(&mut reader)?),
            profile_artifact: ArtifactDigest::from_sha256(digest(&mut reader)?),
        },
        2 => {
            let plan_id = EvaluationPlanId::new(reader.read_fixed().map_err(codec)?)?;
            let plan_digest = PlanDigest::new(digest(&mut reader)?);
            let ordinal = reader.read_u32().map_err(codec)?;
            let total = reader.read_u32().map_err(codec)?;
            let artifact = ArtifactDigest::from_sha256(digest(&mut reader)?);
            let length = reader.read_collection_len().map_err(codec)?;
            let mut bindings = Vec::with_capacity(length);
            for _ in 0..length {
                bindings.push(PlannedRolloutBinding::new(
                    RolloutId::new(reader.read_fixed().map_err(codec)?)?,
                    WorkId::new(reader.read_fixed().map_err(codec)?)
                        .map_err(|_| protocol("invalid D3 work identity"))?,
                    digest(&mut reader)?,
                ));
            }
            EvaluationCommandKind::RecordPlanBatch {
                plan_id,
                plan_digest,
                batch: PlanBatch::new(ordinal, total, artifact, bindings)?,
            }
        }
        3 => EvaluationCommandKind::CompletePlan { plan: plan(&mut reader)? },
        4 => EvaluationCommandKind::RequestSchedule {
            rollout_id: rollout(&mut reader)?,
            work: decode_work(&mut reader)?,
        },
        5 => EvaluationCommandKind::RecordSchedule {
            rollout_id: rollout(&mut reader)?,
            acknowledgement_digest: digest(&mut reader)?,
        },
        6 => EvaluationCommandKind::StartRollout {
            rollout_id: rollout(&mut reader)?,
            attempt: reader.read_u16().map_err(codec)?,
            started_at_tick: reader.read_u64().map_err(codec)?,
        },
        7 => EvaluationCommandKind::RetainRetryableAttempt {
            rollout_id: rollout(&mut reader)?,
            attempt: reader.read_u16().map_err(codec)?,
            observation_digest: digest(&mut reader)?,
        },
        8 => EvaluationCommandKind::SettleRollout {
            rollout_id: rollout(&mut reader)?,
            terminal: terminal(&mut reader)?,
        },
        9 => EvaluationCommandKind::CancelCampaign { reason_digest: digest(&mut reader)? },
        10 => EvaluationCommandKind::CompleteCancellation,
        11 => EvaluationCommandKind::StartAnalysis { counts: counts(&mut reader)? },
        12 => EvaluationCommandKind::CompleteAnalysis {
            analysis_digest: ResultDigest::new(digest(&mut reader)?),
            artifact: ArtifactDigest::from_sha256(digest(&mut reader)?),
            artifact_bytes: reader.read_u64().map_err(codec)?,
        },
        13 => EvaluationCommandKind::CompleteReport { report: report(&mut reader)? },
        14 => EvaluationCommandKind::RecordPublication {
            publication: PublicationRecord::new(
                EvaluationReportId::new(reader.read_fixed().map_err(codec)?)?,
                EvidenceId::new(reader.read_fixed().map_err(codec)?)
                    .map_err(|_| protocol("invalid evidence identity"))?,
                reader.read_u64().map_err(codec)?,
            )?,
        },
        15 => EvaluationCommandKind::FailCampaign {
            failure: CampaignFailure::new(
                CampaignFailureCode::from_tag(reader.read_u8().map_err(codec)?)?,
                digest(&mut reader)?,
            ),
        },
        16 => EvaluationCommandKind::SettleCancellation {
            rollout_id: rollout(&mut reader)?,
            observation_digest: digest(&mut reader)?,
        },
        _ => return Err(protocol("unknown evaluation semantic tag")),
    };
    reader.finish().map_err(codec)?;
    Ok(kind)
}

fn revision(reader: &mut CanonicalReader<'_>) -> Result<RevisionTuple, EvaluationError> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid acceptance identity"))?,
        HarnessId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid harness identity"))?,
        WorkspaceId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace identity"))?,
        Generation::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace generation"))?,
        RevisionNumber::new(reader.read_u64().map_err(codec)?)
            .map_err(|_| protocol("invalid workspace revision"))?,
        PolicyId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid policy identity"))?,
        ProviderProfileId::new(reader.read_fixed().map_err(codec)?)
            .map_err(|_| protocol("invalid provider identity"))?,
    ))
}
fn plan(reader: &mut CanonicalReader<'_>) -> Result<PlanRecord, EvaluationError> {
    PlanRecord::new(
        EvaluationPlanId::new(reader.read_fixed().map_err(codec)?)?,
        PlanDigest::new(digest(reader)?),
        ArtifactDigest::from_sha256(digest(reader)?),
        reader.read_u32().map_err(codec)?,
        reader.read_u32().map_err(codec)?,
    )
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "canonical work decoder is also consumed by the directive codec"
)]
pub(crate) fn decode_work(reader: &mut CanonicalReader<'_>) -> Result<WorkSpec, EvaluationError> {
    let id = WorkId::new(reader.read_fixed().map_err(codec)?)
        .map_err(|_| protocol("invalid work identity"))?;
    let owner = ActorId::new(reader.read_fixed().map_err(codec)?)
        .map_err(|_| protocol("invalid work owner identity"))?;
    let revision = revision(reader)?;
    let class = match reader.read_u8().map_err(codec)? {
        1 => ExecutionClass::Model,
        2 => ExecutionClass::Tool,
        3 => ExecutionClass::Gate,
        4 => ExecutionClass::Review,
        5 => ExecutionClass::Coordination,
        _ => return Err(protocol("unknown execution class")),
    };
    let priority = reader.read_u8().map_err(codec)?;
    let resource_len = reader.read_collection_len().map_err(codec)?;
    let mut resources = Vec::with_capacity(resource_len);
    for _ in 0..resource_len {
        resources.push(ResourceEntry::new(
            ResourceKind::new(reader.read_u16().map_err(codec)?)
                .map_err(|_| protocol("invalid resource kind"))?,
            ResourceQuantity::new(reader.read_u64().map_err(codec)?)
                .map_err(|_| protocol("invalid resource quantity"))?,
        ));
    }
    let request = ResourceVector::new(resources, SchedulerLimits::MAX_RESOURCE_DIMENSIONS)
        .map_err(|_| protocol("invalid resource vector"))?;
    let budget_reservation = reader
        .read_option_tag()
        .map_err(codec)?
        .then(|| {
            BudgetReservationId::new(reader.read_fixed().map_err(codec)?)
                .map_err(|_| protocol("invalid budget reservation identity"))
        })
        .transpose()?;
    let dependency_len = reader.read_collection_len().map_err(codec)?;
    let mut dependencies = Vec::with_capacity(dependency_len);
    for _ in 0..dependency_len {
        dependencies.push(
            WorkId::new(reader.read_fixed().map_err(codec)?)
                .map_err(|_| protocol("invalid dependency identity"))?,
        );
    }
    let parent = reader
        .read_option_tag()
        .map_err(codec)?
        .then(|| {
            WorkId::new(reader.read_fixed().map_err(codec)?)
                .map_err(|_| protocol("invalid parent work identity"))
        })
        .transpose()?;
    let maximum_attempts = AttemptNumber::new(reader.read_u16().map_err(codec)?)
        .map_err(|_| protocol("invalid maximum attempt count"))?;
    let recovery = match reader.read_u8().map_err(codec)? {
        1 => RecoveryPolicy::RetrySafe,
        2 => RecoveryPolicy::Ambiguous,
        3 => RecoveryPolicy::Fail,
        _ => return Err(protocol("unknown recovery policy")),
    };
    let payload_digest = digest(reader)?;
    WorkSpec::new(
        id,
        owner,
        revision,
        class,
        priority,
        request,
        budget_reservation,
        dependencies,
        parent,
        maximum_attempts,
        recovery,
        payload_digest,
        scheduler_limits(),
    )
    .map_err(|_| protocol("invalid complete work specification"))
}

fn scheduler_limits() -> SchedulerLimits {
    SchedulerLimits::new(
        SchedulerLimits::MAX_QUEUED_WORK,
        SchedulerLimits::MAX_RETAINED_WORK,
        SchedulerLimits::MAX_WORKERS,
        SchedulerLimits::MAX_DEPENDENCIES,
        SchedulerLimits::MAX_RESOURCE_DIMENSIONS,
        SchedulerLimits::MAX_ACTIVE_RESERVATIONS,
        SchedulerLimits::MAX_ATTEMPTS,
        SchedulerLimits::MAX_BYPASS_COUNT,
        SchedulerLimits::MAX_DISPATCH_BATCH,
        SchedulerLimits::MAX_PAYLOAD_BYTES,
        SchedulerLimits::MAX_STATE_BYTES,
    )
    .expect("compiled scheduler maxima are internally valid")
}
fn terminal(reader: &mut CanonicalReader<'_>) -> Result<TerminalRecordRef, EvaluationError> {
    TerminalRecordRef::new(
        RolloutTerminalClass::from_tag(reader.read_u8().map_err(codec)?)?,
        digest(reader)?,
        ArtifactDigest::from_sha256(digest(reader)?),
        reader.read_u64().map_err(codec)?,
        reader.read_u16().map_err(codec)?,
    )
}
fn counts(reader: &mut CanonicalReader<'_>) -> Result<LedgerCounts, EvaluationError> {
    Ok(LedgerCounts {
        expected: reader.read_u32().map_err(codec)?,
        passed: reader.read_u32().map_err(codec)?,
        task_failed: reader.read_u32().map_err(codec)?,
        infrastructure_failed: reader.read_u32().map_err(codec)?,
        cancelled: reader.read_u32().map_err(codec)?,
        ambiguous: reader.read_u32().map_err(codec)?,
    })
}
fn report(reader: &mut CanonicalReader<'_>) -> Result<ReportRecord, EvaluationError> {
    ReportRecord::new(
        EvaluationReportId::new(reader.read_fixed().map_err(codec)?)?,
        digest(reader)?,
        ArtifactDigest::from_sha256(digest(reader)?),
        reader.read_u64().map_err(codec)?,
    )
}
fn rollout(reader: &mut CanonicalReader<'_>) -> Result<RolloutId, EvaluationError> {
    RolloutId::new(reader.read_fixed().map_err(codec)?)
}
fn digest(reader: &mut CanonicalReader<'_>) -> Result<Sha256Digest, EvaluationError> {
    Ok(Sha256Digest::new(reader.read_fixed().map_err(codec)?))
}
const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Corruption,
        EvaluationOperation::Codec,
        EvaluationRecovery::Quarantine,
        "evaluation semantic payload violates canonical codec bounds",
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
