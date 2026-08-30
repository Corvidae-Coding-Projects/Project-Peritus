//! Cross-file invariants for retained campaign evidence.

use super::model::{
    AgentIdentity, CampaignMode, IdentityPolicy, JobState, ReportRequest, TrialReport,
};
use crate::BenchmarkError;

pub(super) fn request(request: &ReportRequest) -> Result<(), BenchmarkError> {
    if request.expected_trials == 0 {
        return Err(arguments("expected trial count must be positive"));
    }
    let label = request.campaign_label.trim();
    if label.is_empty() || label.len() > 128 {
        return Err(arguments("campaign label must contain 1 through 128 bytes"));
    }
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
) -> Result<AgentIdentity, BenchmarkError> {
    let mut source_revision: Option<String> = None;
    let mut native_reports = 0;
    let mut native_reports_with_source_identity = 0;
    let mut native_reports_with_binary_identity = 0;
    for trial in trials {
        native_reports += usize::from(trial.native.is_some());
        let Some(metadata) = &trial.usage.metadata else {
            require_native_identity(request, trial, "has no Harbor identity metadata")?;
            continue;
        };
        if let Some(revision) = &metadata.agent_source_revision {
            validate_hex("reported agent source revision", revision, &[40, 64])?;
            native_reports_with_source_identity += usize::from(trial.native.is_some());
            match &source_revision {
                Some(expected) if revision != expected => {
                    return Err(BenchmarkError::Identity(format!(
                        "trial {} reports source revision {revision}, expected {expected}",
                        trial.trial_name
                    )));
                }
                None => source_revision = Some(revision.clone()),
                Some(_) => {}
            }
        } else {
            require_native_identity(request, trial, "has no source revision")?;
        }
        if let Some(digest) = &metadata.agent_binary_sha256 {
            validate_hex("reported agent SHA-256", digest, &[64])?;
            native_reports_with_binary_identity += usize::from(trial.native.is_some());
            if digest != &request.agent_sha256 {
                return Err(BenchmarkError::Identity(format!(
                    "trial {} reports binary digest {digest}, expected {}",
                    trial.trial_name, request.agent_sha256
                )));
            }
        } else {
            require_native_identity(request, trial, "has no binary digest")?;
        }
    }
    Ok(AgentIdentity {
        source_revision,
        binary_sha256: request.agent_sha256.clone(),
        identity_policy: request.identity_policy,
        native_reports,
        native_reports_with_source_identity,
        native_reports_with_binary_identity,
    })
}

fn require_native_identity(
    request: &ReportRequest,
    trial: &TrialReport,
    detail: &str,
) -> Result<(), BenchmarkError> {
    if request.identity_policy == IdentityPolicy::RequireNative && trial.native.is_some() {
        return Err(BenchmarkError::Identity(format!(
            "native trial {} {detail}",
            trial.trial_name
        )));
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
