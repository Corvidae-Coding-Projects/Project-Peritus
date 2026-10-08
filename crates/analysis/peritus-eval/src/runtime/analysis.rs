//! Durable owner-bound execution of bounded deterministic analysis batches.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, Publication, WriteRequest,
};
use peritus_journal::{JournalCancellation, SqliteJournal};
use peritus_types::{ActorId, EventId};

use crate::{
    AnalysisSafePoint, BootstrapBatchWork, BootstrapCursor, EvaluationAnalysis,
    EvaluationAnalysisBatch, EvaluationAnalysisCheckpoint, EvaluationCommandKind, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationPhase, EvaluationPlan, EvaluationRecovery,
    EvaluationState, FrozenEvaluationProfile, RolloutLedger, analyze_evaluation_batch,
};

use super::{CommittedEvaluationTransition, EvaluationRuntime, TransitionIds};

/// Verified canonical analysis artifact committed by `CompleteAnalysis`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedAnalysisArtifact {
    digest: ArtifactDigest,
    size: u64,
    publication: Publication,
}

impl FinalizedAnalysisArtifact {
    /// Content-addressed artifact identity.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }
    /// Exact canonical byte length.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Whether this turn created or reused the exact content.
    #[must_use]
    pub const fn publication(self) -> Publication {
        self.publication
    }
}

/// Durable outcome of one physical analysis turn.
#[derive(Debug)]
pub enum DurableAnalysisAdvance {
    /// The exact frontier was finalized and committed before returning control.
    Checkpointed {
        /// New owner-bound durable safe point.
        safe_point: AnalysisSafePoint,
        /// Exact bootstrap position, absent for a metric with no bootstrap work.
        cursor: Option<BootstrapCursor>,
        /// Whether cancellation rather than the physical work budget caused the yield.
        cancelled: bool,
        /// Accepted C0 checkpoint or cancellation-settlement transition.
        committed: CommittedEvaluationTransition,
    },
    /// Analysis completed and its exact canonical artifact was committed.
    Complete {
        /// Complete deterministic analysis values.
        analysis: EvaluationAnalysis,
        /// Verified finalized canonical analysis artifact.
        artifact: FinalizedAnalysisArtifact,
        /// Accepted `CompleteAnalysis` transition.
        committed: CommittedEvaluationTransition,
    },
}

/// Advances one exact campaign analysis batch and durably records every yield boundary.
///
/// Existing safe-point bytes are loaded through their content-addressed artifact identity and
/// rebound to the supplied immutable analysis inputs before any draw is performed.
///
/// # Errors
/// Rejects phase, owner, input, checkpoint, cancellation, artifact, or C0 transition drift.
#[allow(
    clippy::too_many_arguments,
    reason = "durable analysis binds every owner, input, work, cancellation, and transition identity"
)]
pub fn advance_durable_analysis(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    plan: &EvaluationPlan,
    profile: &FrozenEvaluationProfile,
    ledger: &RolloutLedger,
    owner: ActorId,
    work: BootstrapBatchWork,
    cancellation: &JournalCancellation,
    ids: TransitionIds,
) -> Result<DurableAnalysisAdvance, EvaluationError> {
    let cancelling = state.phase() == EvaluationPhase::Cancelling
        && state.cancellation_origin() == Some(EvaluationPhase::Analyzing)
        && !state.analysis_cancellation_settled();
    if state.phase() != EvaluationPhase::Analyzing && !cancelling {
        return Err(binding("analysis batch state is not active or cancelling analysis"));
    }
    if cancelling && !cancellation.is_cancelled() {
        return Err(binding("analysis cancellation settlement has no observed cancellation"));
    }
    if state.analysis_digest().is_some()
        || plan.campaign_id() != state.campaign_id()
        || profile.digest() != state.profile_digest()
        || state.plan().is_none_or(|retained| {
            retained.id() != plan.id() || retained.digest() != plan.digest()
        })
        || !ledger.complete()
        || state.analysis_counts() != Some(ledger.counts())
    {
        return Err(binding("analysis state, profile, plan, or complete ledger binding differs"));
    }
    let prior = load_checkpoint(artifact_store, state, owner)?;
    let batch = analyze_evaluation_batch(
        plan,
        profile,
        ledger,
        prior.as_ref(),
        work,
        || cancellation.is_cancelled(),
    )?;
    match batch {
        EvaluationAnalysisBatch::Checkpoint { checkpoint, cursor, cancelled } => {
            if cancelling && !cancelled {
                return Err(binding("cancelling analysis advanced beyond its retained frontier"));
            }
            let finalized = finalize_payload(
                artifact_store,
                checkpoint.bytes(),
                ids.event_id(),
                "application/vnd.peritus.evaluation-analysis-checkpoint+binary",
            )?;
            let checkpoint_digest = peritus_codec::sha256(checkpoint.bytes());
            if finalized.digest().sha256() != checkpoint_digest {
                return Err(artifact("finalized analysis checkpoint digest differs"));
            }
            let sequence = state
                .analysis_safe_point()
                .map_or(Some(1), |current| current.sequence().checked_add(1))
                .ok_or_else(|| binding("analysis safe-point sequence overflowed"))?;
            let safe_point = AnalysisSafePoint::new(
                owner,
                sequence,
                checkpoint_digest,
                finalized.digest(),
                finalized.size(),
            )?;
            let committed = if cancelling {
                EvaluationRuntime::new(journal)
                    .settle_analysis_cancellation(state, safe_point, ids)?
            } else {
                EvaluationRuntime::new(journal)
                    .record_analysis_safe_point(state, safe_point, ids)?
            };
            Ok(DurableAnalysisAdvance::Checkpointed {
                safe_point,
                cursor,
                cancelled,
                committed,
            })
        }
        EvaluationAnalysisBatch::Complete(analysis) => {
            if cancelling {
                return Err(binding("cancelled analysis cannot commit a completed result"));
            }
            let bytes = analysis.canonical_bytes(profile.digest())?;
            let result_digest = analysis.digest();
            if result_digest.digest() != peritus_codec::sha256(&bytes) {
                return Err(binding("analysis digest differs from canonical artifact bytes"));
            }
            let finalized = finalize_payload(
                artifact_store,
                &bytes,
                ids.event_id(),
                "application/vnd.peritus.evaluation-analysis+binary",
            )?;
            let artifact = FinalizedAnalysisArtifact {
                digest: finalized.digest(),
                size: finalized.size(),
                publication: finalized.publication(),
            };
            let kind = EvaluationCommandKind::CompleteAnalysis {
                analysis_digest: result_digest,
                artifact: artifact.digest(),
                artifact_bytes: artifact.size(),
            };
            let committed = EvaluationRuntime::new(journal).commit_kind(state, ids, kind)?;
            Ok(DurableAnalysisAdvance::Complete { analysis, artifact, committed })
        }
    }
}

fn load_checkpoint(
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    owner: ActorId,
) -> Result<Option<EvaluationAnalysisCheckpoint>, EvaluationError> {
    let Some(safe_point) = state.analysis_safe_point() else {
        return Ok(None);
    };
    if safe_point.owner() != owner
        || safe_point.artifact().sha256() != safe_point.checkpoint_digest()
    {
        return Err(binding("analysis safe-point owner or artifact identity differs"));
    }
    let bytes = artifact_store
        .read(safe_point.artifact(), safe_point.artifact_bytes())
        .map_err(artifact_owner)?;
    if u64::try_from(bytes.len()).map_err(|_| artifact("analysis checkpoint size overflowed"))?
        != safe_point.artifact_bytes()
        || peritus_codec::sha256(&bytes) != safe_point.checkpoint_digest()
    {
        return Err(artifact("analysis checkpoint bytes differ from the durable safe point"));
    }
    Ok(Some(EvaluationAnalysisCheckpoint::from_artifact_bytes(bytes)?))
}

fn finalize_payload(
    store: &ArtifactStore,
    bytes: &[u8],
    creating_event: EventId,
    media_type: &'static str,
) -> Result<peritus_artifact_store::FinalizedArtifact, EvaluationError> {
    let size = u64::try_from(bytes.len()).map_err(|_| artifact("analysis artifact size overflowed"))?;
    if size == 0 {
        return Err(artifact("analysis artifact is empty"));
    }
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let media_type = MediaType::new(media_type)
        .map_err(|_| artifact("analysis artifact media type is invalid"))?;
    let request = WriteRequest::new(
        digest,
        size,
        size,
        media_type,
        EncryptionMetadata::unencrypted(),
        creating_event,
    );
    let mut writer = store.begin_write(request).map_err(artifact_owner)?;
    writer.write_chunk(bytes).map_err(artifact_owner)?;
    let finalized = writer.finalize().map_err(artifact_owner)?;
    let metadata = store.verify(finalized.digest()).map_err(artifact_owner)?;
    if finalized.digest() != digest || finalized.size() != size || metadata.size() != size {
        return Err(artifact("finalized analysis artifact differs from canonical bytes"));
    }
    Ok(finalized)
}

fn artifact_owner(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Artifact,
        EvaluationOperation::Analyze,
        EvaluationRecovery::Reconcile,
        "artifact owner failed to finalize or load analysis state",
    )
}
const fn artifact(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Artifact,
        EvaluationOperation::Analyze,
        EvaluationRecovery::Reconcile,
        detail,
    )
}
const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Analyze,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
