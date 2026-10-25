//! Bounded messages for genuine native candidate campaign effects.

use super::{AuthorityHandle, AuthorityMessage};
use crate::DaemonError;
use peritus_eval::{
    EvaluationCampaignId, EvaluationDirectiveClaim, EvaluationState, ExecutionDirectiveClaim,
    FrozenEvaluationProfile, PublicationDirectiveClaim, RolloutRecord, ScheduleDirectiveClaim,
};
use tokio::sync::oneshot;

impl AuthorityHandle {
    pub(crate) async fn publish_debugger(
        &self,
        claim: peritus_debugger::PublicationDirectiveClaim,
        database: std::path::PathBuf,
        state_root: std::path::PathBuf,
        maximum_bytes: u64,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::PublishDebugger {
                claim,
                database,
                state_root,
                maximum_bytes,
                respond,
            },
            receive,
        )
        .await
    }
    /// Admits a fully frozen evaluation through the sole C0 owner.
    pub async fn start_evaluation(
        &self,
        campaign: EvaluationCampaignId,
        profile: FrozenEvaluationProfile,
    ) -> Result<EvaluationState, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::StartEvaluation { campaign, profile: Box::new(profile), respond },
            receive,
        )
        .await
    }
    /// Reads complete replayed campaign state without dispatching effects.
    pub async fn evaluation(
        &self,
        campaign: EvaluationCampaignId,
    ) -> Result<Option<EvaluationState>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::QueryEvaluation { campaign, respond }, receive).await
    }
    /// Commits cancellation before the worker observes it.
    pub async fn cancel_evaluation(
        &self,
        campaign: EvaluationCampaignId,
        reason: peritus_types::Sha256Digest,
    ) -> Result<EvaluationState, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::CancelEvaluation { campaign, reason, respond }, receive).await
    }
    pub(crate) async fn deliver_evaluation_schedule(
        &self,
        claim: ScheduleDirectiveClaim,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::DeliverEvaluationSchedule { claim, respond }, receive).await
    }
    pub(crate) async fn begin_evaluation(
        &self,
        claim: ExecutionDirectiveClaim,
        cancellation: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Option<crate::rsi::authority::EvaluationWork>, DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::BeginEvaluation { claim, cancellation, respond }, receive).await
    }
    pub(crate) async fn finish_evaluation(
        &self,
        claim: ExecutionDirectiveClaim,
        record: RolloutRecord,
        processes: peritus_process::ProcessStore,
        state_root: std::path::PathBuf,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::FinishEvaluation { claim, record, processes, state_root, respond },
            receive,
        )
        .await
    }
    pub(crate) async fn retain_evaluation_artifacts(
        &self,
        claim: ExecutionDirectiveClaim,
        roots: Vec<peritus_artifact_store::ArtifactDigest>,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::RetainEvaluationArtifacts { claim, roots, respond }, receive)
            .await
    }
    pub(crate) async fn publish_evaluation(
        &self,
        claim: PublicationDirectiveClaim,
        database: std::path::PathBuf,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(AuthorityMessage::PublishEvaluation { claim, database, respond }, receive).await
    }
    pub(crate) async fn fail_evaluation(
        &self,
        claim: EvaluationDirectiveClaim,
        detail: String,
        processes: peritus_process::ProcessStore,
        config: crate::DaemonConfig,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::FailEvaluation {
                claim,
                detail,
                processes,
                config: Box::new(config),
                respond,
            },
            receive,
        )
        .await
    }
    pub(crate) async fn publish_evolution(
        &self,
        claim: peritus_evolution::EvolutionPublicationClaim,
        database: std::path::PathBuf,
        maximum_bytes: u64,
    ) -> Result<(), DaemonError> {
        let (respond, receive) = oneshot::channel();
        self.send(
            AuthorityMessage::PublishEvolution { claim, database, maximum_bytes, respond },
            receive,
        )
        .await
    }
}
