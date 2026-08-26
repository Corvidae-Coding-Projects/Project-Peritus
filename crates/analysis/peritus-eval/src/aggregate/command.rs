//! Closed CAS-fenced evaluation command vocabulary.

use peritus_codec::{CanonicalWriter, CodecLimits};
use peritus_types::{CommandId, EventId, RevisionTuple, Sha256Digest};

use crate::{
    CampaignFailure, DatasetDigest, EvaluationCampaignId, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationRecovery, LedgerCounts, PlanBatch, PlanRecord, ProfileDigest,
    PublicationRecord, ReportRecord, ResultDigest, RolloutId, TerminalRecordRef,
};

const COMMAND_DOMAIN: &[u8] = b"peritus.evaluation.command.v1\0";

/// Closed E3 command vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluationCommandKind {
    /// Registers immutable campaign inputs.
    CreateCampaign {
        /// Cross-slice provenance revision.
        revision: RevisionTuple,
        /// Complete dataset manifest digest.
        dataset_digest: DatasetDigest,
        /// Finalized canonical dataset manifest artifact.
        dataset_artifact: peritus_artifact_store::ArtifactDigest,
        /// Finalized canonical frozen-profile artifact.
        profile_artifact: peritus_artifact_store::ArtifactDigest,
    },
    /// Appends one canonical artifact-backed plan batch.
    RecordPlanBatch {
        /// Complete plan identity.
        plan_id: crate::EvaluationPlanId,
        /// Complete plan digest.
        plan_digest: crate::PlanDigest,
        /// Next exact bounded batch.
        batch: PlanBatch,
    },
    /// Finalizes the complete plan root after every batch.
    CompletePlan {
        /// Plan root and exact cardinality.
        plan: PlanRecord,
    },
    /// Creates the exact D3 schedule directive for one planned rollout.
    RequestSchedule {
        /// Planned rollout identity.
        rollout_id: RolloutId,
        /// Complete inert D3 work specification.
        work: peritus_scheduler::WorkSpec,
    },
    /// Records D3 acknowledgement of the exact work identity.
    RecordSchedule {
        /// Scheduled rollout identity.
        rollout_id: RolloutId,
        /// Exact D3 acknowledgement digest.
        acknowledgement_digest: Sha256Digest,
    },
    /// Commits attempt start before any candidate/evaluator effect.
    StartRollout {
        /// Scheduled rollout identity.
        rollout_id: RolloutId,
        /// One-based monotonic attempt.
        attempt: u16,
        /// Positive caller monotonic start tick.
        started_at_tick: u64,
    },
    /// Retains a retryable attempt artifact before returning to scheduled state.
    RetainRetryableAttempt {
        /// Executed rollout identity.
        rollout_id: RolloutId,
        /// Exact attempt number.
        attempt: u16,
        /// Complete retained attempt digest.
        observation_digest: Sha256Digest,
    },
    /// Settles one logical terminal and its exact result artifact.
    SettleRollout {
        /// Executed rollout identity.
        rollout_id: RolloutId,
        /// Complete terminal artifact reference.
        terminal: TerminalRecordRef,
    },
    /// Durably starts cancellation before external routing.
    CancelCampaign {
        /// Bounded redaction-safe reason digest.
        reason_digest: Sha256Digest,
    },
    /// Records reconciliation of one exact schedule or execution cancellation directive.
    SettleCancellation {
        /// Rollout whose owned external work is now cancelled.
        rollout_id: RolloutId,
        /// Redaction-safe digest of the external cancellation observation.
        observation_digest: Sha256Digest,
    },
    /// Finalizes cancellation after every unsettled rollout is reconciled.
    CompleteCancellation,
    /// Starts analysis only with exact complete conservation counts.
    StartAnalysis {
        /// Complete logical terminal counts.
        counts: LedgerCounts,
    },
    /// Commits deterministic analysis output as a finalized artifact.
    CompleteAnalysis {
        /// Digest of every computed value.
        analysis_digest: ResultDigest,
        /// Exact finalized analysis artifact.
        artifact: peritus_artifact_store::ArtifactDigest,
        /// Exact artifact byte length.
        artifact_bytes: u64,
    },
    /// Commits canonical report bytes before publication.
    CompleteReport {
        /// Exact report artifact record.
        report: ReportRecord,
    },
    /// Records admitted evidence and completes publication.
    RecordPublication {
        /// Exact report/evidence provenance.
        publication: PublicationRecord,
    },
    /// Terminates a nonterminal campaign with a typed failure.
    FailCampaign {
        /// Redaction-safe failure record.
        failure: CampaignFailure,
    },
}

/// One exact fenced command for an evaluation aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationCommand {
    command_id: CommandId,
    event_id: EventId,
    campaign_id: EvaluationCampaignId,
    expected_sequence: u64,
    expected_previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    profile_digest: ProfileDigest,
    kind: EvaluationCommandKind,
    digest: Sha256Digest,
}

impl EvaluationCommand {
    /// Constructs a command binding every CAS and semantic field.
    ///
    /// # Errors
    /// Rejects inconsistent sequence/predecessor fences or invalid payloads.
    #[allow(clippy::too_many_arguments, reason = "all aggregate fence fields remain explicit")]
    pub fn new(
        command_id: CommandId,
        event_id: EventId,
        campaign_id: EvaluationCampaignId,
        expected_sequence: u64,
        expected_previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        profile_digest: ProfileDigest,
        kind: EvaluationCommandKind,
    ) -> Result<Self, EvaluationError> {
        if (expected_sequence == 0) != expected_previous_event.is_none() {
            return Err(invalid("command sequence and predecessor presence disagree"));
        }
        validate_kind(&kind)?;
        let mut command = Self {
            command_id,
            event_id,
            campaign_id,
            expected_sequence,
            expected_previous_event,
            prior_state_digest,
            profile_digest,
            kind,
            digest: Sha256Digest::new([0; 32]),
        };
        command.digest = peritus_codec::sha256(&command.canonical_identity_bytes()?);
        Ok(command)
    }
    /// Command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Reserved event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Campaign identity.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Expected current sequence.
    #[must_use]
    pub const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    /// Expected aggregate head.
    #[must_use]
    pub const fn expected_previous_event(&self) -> Option<EventId> {
        self.expected_previous_event
    }
    /// Expected complete prior-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Immutable profile digest repeated on every command.
    #[must_use]
    pub const fn profile_digest(&self) -> ProfileDigest {
        self.profile_digest
    }
    /// Semantic command.
    #[must_use]
    pub const fn kind(&self) -> &EvaluationCommandKind {
        &self.kind
    }
    /// Digest of every command field.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) fn canonical_identity_bytes(&self) -> Result<Vec<u8>, EvaluationError> {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        writer.write_fixed(COMMAND_DOMAIN).map_err(codec)?;
        writer.write_fixed(self.command_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.event_id.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.campaign_id.as_bytes()).map_err(codec)?;
        writer.write_u64(self.expected_sequence).map_err(codec)?;
        writer.write_option_tag(self.expected_previous_event.is_some()).map_err(codec)?;
        if let Some(value) = self.expected_previous_event {
            writer.write_fixed(value.as_bytes()).map_err(codec)?;
        }
        writer.write_fixed(self.prior_state_digest.as_bytes()).map_err(codec)?;
        writer.write_fixed(self.profile_digest.as_bytes()).map_err(codec)?;
        super::reducer::encode_kind(&mut writer, &self.kind)?;
        Ok(writer.into_bytes())
    }
}

fn validate_kind(kind: &EvaluationCommandKind) -> Result<(), EvaluationError> {
    match kind {
        EvaluationCommandKind::StartRollout { attempt, started_at_tick, .. }
            if *attempt == 0 || *started_at_tick == 0 =>
        {
            Err(invalid("rollout attempt or start tick is zero"))
        }
        EvaluationCommandKind::RetainRetryableAttempt { attempt, .. } if *attempt == 0 => {
            Err(invalid("retained rollout attempt is zero"))
        }
        EvaluationCommandKind::CompleteAnalysis { artifact_bytes: 0, .. } => {
            Err(invalid("analysis artifact byte length is zero"))
        }
        EvaluationCommandKind::RequestSchedule { rollout_id, work }
            if work.id().as_bytes() == &[0; 16]
                || work.payload_digest().as_bytes() == &[0; 32]
                || work.class() != peritus_scheduler::ExecutionClass::Coordination
                || work.id().as_bytes() == rollout_id.as_bytes() =>
        {
            Err(invalid("evaluation schedule work specification is invalid"))
        }
        _ => Ok(()),
    }
}

const fn codec(_: peritus_codec::CodecError) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::LimitExceeded,
        EvaluationOperation::Codec,
        EvaluationRecovery::ReduceScope,
        "evaluation command exceeds canonical codec limits",
    )
}
const fn invalid(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::ApplyTransition,
        EvaluationRecovery::CorrectInput,
        detail,
    )
}
