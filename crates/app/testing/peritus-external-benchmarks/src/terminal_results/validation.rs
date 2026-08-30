//! Cross-file invariants for retained campaign evidence.

use super::model::{CampaignMode, JobState, ReportRequest, TrialReport};
use crate::BenchmarkError;

pub(super) fn request(request: &ReportRequest) -> Result<(), BenchmarkError> {
    if request.expected_trials == 0 {
        return Err(arguments("expected trial count must be positive"));
    }
    let label = request.campaign_label.trim();
    if label.is_empty() || label.len() > 128 {
        return Err(arguments("campaign label must contain 1 through 128 bytes"));
    }
    validate_hex("agent source revision", &request.agent_source_revision, &[40, 64])?;
    validate_hex("agent SHA-256", &request.agent_sha256, &[64])?;
    if request.output.exists() {
        return Err(BenchmarkError::Workspace(format!(
            "report output already exists: {}",
            request.output.display()
        )));
    }
    Ok(())
}

pub(super) fn state(
    request: &ReportRequest,
    state: &JobState,
    child_results: usize,
) -> Result<(), BenchmarkError> {
    if state.n_total_trials != request.expected_trials {
        return Err(BenchmarkError::Workspace(format!(
            "job declares {} trials, expected {}",
            state.n_total_trials, request.expected_trials
        )));
    }
    if state.stats.completed != child_results {
        return Err(BenchmarkError::Workspace(format!(
            "job state reports {} completed trials but {} child result files are visible",
            state.stats.completed, child_results
        )));
    }
    if request.mode == CampaignMode::Final
        && (state.finished_at.is_none()
            || child_results != request.expected_trials
            || state.stats.running != 0
            || state.stats.pending != 0
            || state.stats.cancelled != 0)
    {
        return Err(BenchmarkError::Workspace(
            "final report requires a finished job with every expected child result visible"
                .to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn trial_identity(
    request: &ReportRequest,
    trials: &[TrialReport],
) -> Result<(), BenchmarkError> {
    for trial in trials {
        let Some(metadata) = &trial.usage.metadata else {
            continue;
        };
        if let Some(revision) = &metadata.agent_source_revision
            && revision != &request.agent_source_revision
        {
            return Err(BenchmarkError::Identity(format!(
                "trial {} reports source revision {revision}, expected {}",
                trial.trial_name, request.agent_source_revision
            )));
        }
        if let Some(digest) = &metadata.agent_binary_sha256
            && digest != &request.agent_sha256
        {
            return Err(BenchmarkError::Identity(format!(
                "trial {} reports binary digest {digest}, expected {}",
                trial.trial_name, request.agent_sha256
            )));
        }
    }
    Ok(())
}

fn validate_hex(label: &str, value: &str, lengths: &[usize]) -> Result<(), BenchmarkError> {
    if lengths.contains(&value.len())
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(arguments(format!("{label} is not a full lowercase hexadecimal identity")))
}

fn arguments(detail: impl Into<String>) -> BenchmarkError {
    BenchmarkError::Arguments(detail.into())
}
