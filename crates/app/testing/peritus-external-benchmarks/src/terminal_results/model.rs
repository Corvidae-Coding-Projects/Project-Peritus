//! Versioned campaign-report model and deterministic aggregation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::BenchmarkError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CampaignMode {
    Snapshot,
    Final,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IdentityPolicy {
    AllowLegacy,
    RequireNative,
}

#[derive(Clone, Debug)]
pub(super) struct ReportRequest {
    pub(super) job_directory: PathBuf,
    pub(super) output: PathBuf,
    pub(super) pin_file: PathBuf,
    pub(super) expected_trials: usize,
    pub(super) mode: CampaignMode,
    pub(super) campaign_label: String,
    pub(super) identity_policy: IdentityPolicy,
    pub(super) agent_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CampaignReport {
    pub(super) schema_version: u32,
    pub(super) campaign_label: String,
    pub(super) mode: CampaignMode,
    pub(super) complete: bool,
    pub(super) job_directory: PathBuf,
    pub(super) job_id: String,
    pub(super) started_at: String,
    pub(super) updated_at: String,
    pub(super) finished_at: Option<String>,
    pub(super) expected_trials: usize,
    pub(super) declared_trials: usize,
    pub(super) state: JobCounts,
    pub(super) pin: PinEvidence,
    pub(super) agent: AgentIdentity,
    pub(super) aggregate: Aggregate,
    pub(super) trials: Vec<TrialReport>,
}

/// Small machine-readable confirmation emitted after a report is published.
#[derive(Clone, Debug, Serialize)]
pub struct PublishedSummary {
    output: PathBuf,
    complete: bool,
    completed_trials: usize,
    reward_one: usize,
    reward_zero: usize,
    unscored: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct JobCounts {
    #[serde(rename = "n_completed_trials")]
    pub(super) completed: usize,
    #[serde(rename = "n_errored_trials")]
    pub(super) errored: usize,
    #[serde(rename = "n_running_trials")]
    pub(super) running: usize,
    #[serde(rename = "n_pending_trials")]
    pub(super) pending: usize,
    #[serde(rename = "n_cancelled_trials")]
    pub(super) cancelled: usize,
    #[serde(rename = "n_retries")]
    pub(super) retries: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct PinEvidence {
    pub(super) path: PathBuf,
    pub(super) sha256: String,
    pub(super) contents: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AgentIdentity {
    pub(super) source_revision: Option<String>,
    pub(super) binary_sha256: String,
    pub(super) identity_policy: IdentityPolicy,
    pub(super) native_reports: usize,
    pub(super) native_reports_with_source_identity: usize,
    pub(super) native_reports_with_binary_identity: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Aggregate {
    pub(super) completed_trials: usize,
    pub(super) scored_trials: usize,
    pub(super) reward_one: usize,
    pub(super) reward_zero: usize,
    pub(super) fractional_reward: usize,
    pub(super) unscored: usize,
    pub(super) exception_trials: usize,
    pub(super) native_accepted: usize,
    pub(super) native_rejected: usize,
    pub(super) native_report_missing: usize,
    pub(super) harbor_retries: usize,
    pub(super) native_requests: u64,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) reward_sum: f64,
    pub(super) scored_accuracy: Option<f64>,
    pub(super) completed_success_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TrialReport {
    pub(super) trial_name: String,
    pub(super) task_name: String,
    pub(super) task_ref: String,
    pub(super) source: String,
    pub(super) task_checksum: String,
    pub(super) reward: Option<f64>,
    pub(super) outcome: TrialOutcome,
    pub(super) started_at: String,
    pub(super) finished_at: String,
    pub(super) agent: HarborAgent,
    pub(super) usage: HarborUsage,
    pub(super) exception: Option<ExceptionSummary>,
    pub(super) native: Option<NativeInvocation>,
    pub(super) evidence: TrialEvidencePaths,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TrialOutcome {
    RewardOne,
    RewardZero,
    FractionalReward,
    Unscored,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HarborAgent {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) model_info: HarborModel,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct HarborModel {
    pub(super) name: String,
    pub(super) provider: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct HarborUsage {
    pub(super) n_input_tokens: Option<u64>,
    pub(super) n_cache_tokens: Option<u64>,
    pub(super) n_output_tokens: Option<u64>,
    pub(super) cost_usd: Option<f64>,
    pub(super) metadata: Option<HarborMetadata>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct HarborMetadata {
    #[serde(rename = "peritus_product_accepted")]
    pub(super) product_accepted: Option<bool>,
    #[serde(rename = "peritus_failure_kind")]
    pub(super) failure_kind: Option<String>,
    #[serde(rename = "peritus_requests")]
    pub(super) requests: Option<u64>,
    #[serde(rename = "peritus_agent_source_revision")]
    pub(super) agent_source_revision: Option<String>,
    #[serde(rename = "peritus_agent_binary_sha256")]
    pub(super) agent_binary_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ExceptionSummary {
    pub(super) exception_type: String,
    pub(super) exception_message: String,
    pub(super) occurred_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct NativeInvocation {
    pub(super) schema_version: u32,
    pub(super) success: bool,
    pub(super) task_id: String,
    pub(super) session_id: String,
    pub(super) harness_model_id: String,
    pub(super) writer: String,
    pub(super) reviewer: String,
    pub(super) elapsed_ms: u128,
    pub(super) usage: NativeUsage,
    pub(super) failure_kind: Option<String>,
    pub(super) failure: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct NativeUsage {
    pub(super) requests: u64,
    pub(super) input_tokens: u64,
    pub(super) cached_input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) provider_cost_microunits: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TrialEvidencePaths {
    pub(super) harbor_result: PathBuf,
    pub(super) native_invocation: Option<PathBuf>,
    pub(super) native_trace: Option<PathBuf>,
    pub(super) native_observation: Option<PathBuf>,
    pub(super) verifier_output: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct JobState {
    pub(super) id: String,
    pub(super) started_at: String,
    pub(super) updated_at: String,
    pub(super) finished_at: Option<String>,
    pub(super) n_total_trials: usize,
    pub(super) stats: JobCounts,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct HarborTrial {
    pub(super) trial_name: String,
    pub(super) task_name: String,
    pub(super) task_id: HarborTaskId,
    pub(super) source: String,
    pub(super) task_checksum: String,
    pub(super) agent_info: HarborAgent,
    pub(super) agent_result: HarborUsage,
    pub(super) verifier_result: Option<VerifierResult>,
    pub(super) exception_info: Option<ExceptionSummary>,
    pub(super) started_at: String,
    pub(super) finished_at: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct HarborTaskId {
    pub(super) r#ref: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct VerifierResult {
    pub(super) rewards: Rewards,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Rewards {
    pub(super) reward: Option<f64>,
}

impl ReportRequest {
    pub(super) fn validate(&self) -> Result<(), BenchmarkError> {
        super::validation::request(self)
    }
}

impl CampaignReport {
    pub(super) fn assemble(
        request: &ReportRequest,
        job_directory: &Path,
        pin_file: &Path,
        state: JobState,
        trials: Vec<TrialReport>,
    ) -> Result<Self, BenchmarkError> {
        super::validation::state(request, &state, trials.len())?;
        let agent = super::validation::trial_identity(request, &trials)?;
        let complete = state.finished_at.is_some()
            && state.stats.running == 0
            && state.stats.pending == 0
            && state.stats.cancelled == 0
            && trials.len() == request.expected_trials;
        let pin = super::parse::pin_evidence(pin_file)?;
        let aggregate = Aggregate::from_trials(&trials, state.stats.retries)?;
        Ok(Self {
            schema_version: 1,
            campaign_label: request.campaign_label.clone(),
            mode: request.mode,
            complete,
            job_directory: job_directory.to_path_buf(),
            job_id: state.id,
            started_at: state.started_at,
            updated_at: state.updated_at,
            finished_at: state.finished_at,
            expected_trials: request.expected_trials,
            declared_trials: state.n_total_trials,
            state: state.stats,
            pin,
            agent,
            aggregate,
            trials,
        })
    }
}

impl PublishedSummary {
    pub(super) fn new(output: &Path, report: &CampaignReport) -> Self {
        Self {
            output: output.to_path_buf(),
            complete: report.complete,
            completed_trials: report.aggregate.completed_trials,
            reward_one: report.aggregate.reward_one,
            reward_zero: report.aggregate.reward_zero,
            unscored: report.aggregate.unscored,
        }
    }
}

impl Aggregate {
    fn from_trials(trials: &[TrialReport], harbor_retries: usize) -> Result<Self, BenchmarkError> {
        let mut result = Self { harbor_retries, ..Self::default() };
        for trial in trials {
            result.completed_trials += 1;
            match trial.outcome {
                TrialOutcome::RewardOne => result.reward_one += 1,
                TrialOutcome::RewardZero => result.reward_zero += 1,
                TrialOutcome::FractionalReward => result.fractional_reward += 1,
                TrialOutcome::Unscored => result.unscored += 1,
            }
            if let Some(reward) = trial.reward {
                result.scored_trials += 1;
                result.reward_sum += reward;
            }
            result.exception_trials += usize::from(trial.exception.is_some());
            match trial.native.as_ref().map(|native| native.success) {
                Some(true) => result.native_accepted += 1,
                Some(false) => result.native_rejected += 1,
                None => result.native_report_missing += 1,
            }
            add(
                &mut result.native_requests,
                trial.usage.metadata.as_ref().and_then(|metadata| metadata.requests),
            )?;
            add(&mut result.input_tokens, trial.usage.n_input_tokens)?;
            add(&mut result.cached_input_tokens, trial.usage.n_cache_tokens)?;
            add(&mut result.output_tokens, trial.usage.n_output_tokens)?;
        }
        result.scored_accuracy = ratio(result.reward_sum, result.scored_trials)?;
        result.completed_success_rate = ratio(result.reward_sum, result.completed_trials)?;
        Ok(result)
    }
}

impl TrialOutcome {
    pub(super) fn from_reward(reward: Option<f64>) -> Result<Self, BenchmarkError> {
        match reward {
            None => Ok(Self::Unscored),
            Some(value) if !value.is_finite() || !(0.0..=1.0).contains(&value) => {
                Err(BenchmarkError::Workspace(format!("invalid verifier reward {value}")))
            }
            Some(0.0) => Ok(Self::RewardZero),
            Some(1.0) => Ok(Self::RewardOne),
            Some(_) => Ok(Self::FractionalReward),
        }
    }
}

fn add(total: &mut u64, value: Option<u64>) -> Result<(), BenchmarkError> {
    *total = total
        .checked_add(value.unwrap_or_default())
        .ok_or_else(|| BenchmarkError::Workspace("campaign usage total overflowed".to_owned()))?;
    Ok(())
}

fn ratio(numerator: f64, denominator: usize) -> Result<Option<f64>, BenchmarkError> {
    if denominator == 0 {
        return Ok(None);
    }
    let denominator = u32::try_from(denominator)
        .map_err(|_| BenchmarkError::Workspace("campaign trial count exceeds u32".to_owned()))?;
    Ok(Some(numerator / f64::from(denominator)))
}
