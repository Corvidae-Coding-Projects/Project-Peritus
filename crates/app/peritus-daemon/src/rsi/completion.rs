//! Recovery of committed native work that has no remaining effect directive.

use peritus_artifact_store::ArtifactStore;
use peritus_eval::{
    CampaignFailure, CampaignFailureCode, EvaluationCampaignId, EvaluationCommandKind,
    EvaluationPhase, RolloutStatus,
};
use peritus_journal::{AggregateKind, SqliteJournal};

use super::evaluation as e;
use crate::DaemonError;

/// Resumes actual incomplete admissions and analysis from their complete retained inputs.
pub(crate) fn recover_complete(
    journal: &mut SqliteJournal,
    artifacts: &ArtifactStore,
) -> Result<(), DaemonError> {
    let campaigns = journal.aggregate_ids(AggregateKind::Evaluation, 16384).map_err(e::error)?;
    for campaign in campaigns {
        let campaign = EvaluationCampaignId::new(*campaign.as_bytes()).map_err(e::error)?;
        let state = e::load(journal, campaign)?;
        if e::cancellation_ready(&state) {
            e::ordinary(
                journal,
                &state,
                EvaluationCommandKind::CompleteCancellation,
                "cancel-complete",
            )?;
        } else if matches!(state.phase(), EvaluationPhase::Running | EvaluationPhase::Analyzing)
            && state.counts().complete()
        {
            // The last terminal may already be ACKed. Analysis must not depend on a new claim.
            e::analyze(journal, artifacts, state)?;
        } else if matches!(state.phase(), EvaluationPhase::Created | EvaluationPhase::Planned)
            || (matches!(state.phase(), EvaluationPhase::Scheduling | EvaluationPhase::Running)
                && state.rollouts().any(|(_, p)| matches!(p.status(), RolloutStatus::Planned)))
        {
            match e::profile(artifacts, &state) {
                Ok(profile) => {
                    e::EvaluationOperator::new(journal, artifacts).start(campaign, &profile)?;
                }
                Err(error) => {
                    // No child is executing at this admission boundary. Incomplete legacy inputs
                    // fail explicitly rather than being guessed into executable authority.
                    let event = e::ids(&state, "recover-input-failure")?.event_id();
                    let bytes = serde_json::to_vec(&serde_json::json!({"version":1,
                        "campaign":campaign.as_bytes(), "detail":error.to_string()}))
                    .map_err(e::error)?;
                    let diagnostic = e::save(artifacts, &bytes, event)?;
                    let state = e::retain(journal, artifacts, campaign, vec![diagnostic], None)?;
                    e::ordinary(
                        journal,
                        &state,
                        EvaluationCommandKind::FailCampaign {
                            failure: CampaignFailure::new(
                                CampaignFailureCode::Binding,
                                diagnostic.sha256(),
                            ),
                        },
                        "recover-input-failure",
                    )?;
                }
            }
        }
    }
    Ok(())
}
