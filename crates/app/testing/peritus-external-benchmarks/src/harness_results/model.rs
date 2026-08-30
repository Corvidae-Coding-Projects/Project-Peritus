//! Versioned `HarnessBench` campaign model and deterministic aggregation.

use std::{collections::BTreeSet, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{parse, validation};
use crate::BenchmarkError;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IdentityPolicy {
    AllowLegacy,
    RequireNative,
}

#[derive(Clone, Debug)]
pub(super) struct ReportRequest {
    pub(super) campaign_directory: PathBuf,
    pub(super) task_catalog: PathBuf,
    pub(super) output: PathBuf,
    pub(super) pin_file: PathBuf,
    pub(super) expected_tasks: usize,
    pub(super) campaign_label: String,
    pub(super) identity_policy: IdentityPolicy,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CampaignReport {
    pub(super) schema_version: u32,
    pub(super) campaign_label: String,
    pub(super) selection_policy: &'static str,
    pub(super) complete: bool,
    pub(super) campaign_directory: PathBuf,
    pub(super) results_directory: PathBuf,
    pub(super) task_catalog: PathBuf,
    pub(super) expected_tasks: usize,
    pub(super) selected_tasks: usize,
    pub(super) pin: PinEvidence,
    pub(super) agent: AgentEvidence,
    pub(super) aggregate: Aggregate,
    pub(super) tasks: Vec<TaskReport>,
}

#[derive(Clone, Debug, Serialize)]
/// Small machine-readable confirmation emitted after a report is published.
pub struct PublishedSummary {
    output: PathBuf,
    selected_tasks: usize,
    adapter_successes: usize,
    outcome_mean: f64,
    combined_mean: f64,
    total_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct PinEvidence {
    pub(super) path: PathBuf,
    pub(super) sha256: String,
    pub(super) contents: String,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AgentEvidence {
    pub(super) identity_policy: IdentityPolicy,
    pub(super) native_invocations: usize,
    pub(super) native_invocations_with_identity: usize,
    pub(super) source_revisions: Vec<String>,
    pub(super) binary_sha256s: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(super) struct Aggregate {
    pub(super) tasks: usize,
    pub(super) adapter_successes: usize,
    pub(super) adapter_failures: usize,
    pub(super) outcome_mean: f64,
    pub(super) process_mean: f64,
    pub(super) security_mean: f64,
    pub(super) combined_mean: f64,
    pub(super) perfect_outcomes: usize,
    pub(super) outcomes_at_least_0_9: usize,
    pub(super) combined_at_least_0_9: usize,
    pub(super) elapsed_seconds: f64,
    pub(super) request_count: u64,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_read_tokens: u64,
    pub(super) cache_write_tokens: u64,
    pub(super) total_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TaskReport {
    pub(super) task_id: String,
    pub(super) result_path: PathBuf,
    pub(super) result_sha256: String,
    pub(super) selected_modified_unix_ns: String,
    pub(super) candidate_results: usize,
    pub(super) mode: String,
    pub(super) model_id: String,
    pub(super) api_model_slug: String,
    pub(super) api_model_label: String,
    pub(super) session_id: String,
    pub(super) elapsed_seconds: f64,
    pub(super) adapter_ok: bool,
    pub(super) scores: Scores,
    pub(super) usage: UsageSummary,
    pub(super) evidence: TaskEvidence,
    pub(super) agent_identity: Option<NativeIdentity>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(super) struct Scores {
    #[serde(rename = "outcome_score")]
    pub(super) outcome: f64,
    #[serde(rename = "process_score")]
    pub(super) process: f64,
    #[serde(rename = "security_score")]
    pub(super) security: f64,
    #[serde(rename = "combined_score")]
    pub(super) combined: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct UsageSummary {
    pub(super) available: bool,
    pub(super) source: String,
    pub(super) log_file: PathBuf,
    pub(super) request_count: u64,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_read_tokens: u64,
    pub(super) cache_write_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) providers: Vec<String>,
    pub(super) models: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TaskEvidence {
    pub(super) workspace: PathBuf,
    pub(super) prompt_file: PathBuf,
    pub(super) usage_log: PathBuf,
    pub(super) native_invocation: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct NativeIdentity {
    pub(super) package_version: String,
    pub(super) source_revision: String,
    pub(super) binary_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct UpstreamReport {
    pub(super) task_id: String,
    pub(super) elapsed_sec: f64,
    pub(super) mode: String,
    pub(super) model_id: String,
    pub(super) api_model_slug: String,
    pub(super) api_model_label: String,
    pub(super) session_id: String,
    pub(super) prompt_file: PathBuf,
    pub(super) workspace: PathBuf,
    pub(super) adapter_result: AdapterResult,
    pub(super) usage_summary: UsageSummary,
    pub(super) scoring: Scores,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub(super) struct AdapterResult {
    pub(super) ok: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct NativeInvocation {
    pub(super) task_id: String,
    pub(super) session_id: String,
    pub(super) agent_identity: Option<NativeIdentity>,
}

#[derive(Clone, Debug)]
pub(super) struct SelectedResult {
    pub(super) relative_path: PathBuf,
    pub(super) modified_unix_ns: u128,
    pub(super) candidate_results: usize,
    pub(super) sha256: String,
    pub(super) report: UpstreamReport,
    pub(super) invocation_path: Option<PathBuf>,
    pub(super) invocation: Option<NativeInvocation>,
}

impl CampaignReport {
    pub(super) fn assemble(
        request: &ReportRequest,
        campaign_directory: &std::path::Path,
        task_catalog: &std::path::Path,
        pin_file: &std::path::Path,
        task_names: &BTreeSet<String>,
        selected: Vec<SelectedResult>,
    ) -> Result<Self, BenchmarkError> {
        validation::coverage(request, task_names, &selected)?;
        let pin = parse::pin_evidence(pin_file)?;
        let agent = validation::identities(request, &selected)?;
        let tasks = selected.into_iter().map(TaskReport::from).collect::<Vec<_>>();
        let aggregate = Aggregate::from_tasks(&tasks)?;
        Ok(Self {
            schema_version: 1,
            campaign_label: request.campaign_label.clone(),
            selection_policy: "latest_result_per_task_by_mtime_then_path",
            complete: true,
            campaign_directory: campaign_directory.to_path_buf(),
            results_directory: campaign_directory.join("results"),
            task_catalog: task_catalog.to_path_buf(),
            expected_tasks: request.expected_tasks,
            selected_tasks: tasks.len(),
            pin,
            agent,
            aggregate,
            tasks,
        })
    }
}

impl PublishedSummary {
    pub(super) fn new(output: &std::path::Path, report: &CampaignReport) -> Self {
        Self {
            output: output.to_path_buf(),
            selected_tasks: report.selected_tasks,
            adapter_successes: report.aggregate.adapter_successes,
            outcome_mean: report.aggregate.outcome_mean,
            combined_mean: report.aggregate.combined_mean,
            total_tokens: report.aggregate.total_tokens,
        }
    }
}

impl From<SelectedResult> for TaskReport {
    fn from(selected: SelectedResult) -> Self {
        let report = selected.report;
        Self {
            task_id: report.task_id,
            result_path: selected.relative_path,
            result_sha256: selected.sha256,
            selected_modified_unix_ns: selected.modified_unix_ns.to_string(),
            candidate_results: selected.candidate_results,
            mode: report.mode,
            model_id: report.model_id,
            api_model_slug: report.api_model_slug,
            api_model_label: report.api_model_label,
            session_id: report.session_id,
            elapsed_seconds: report.elapsed_sec,
            adapter_ok: report.adapter_result.ok,
            scores: report.scoring,
            evidence: TaskEvidence {
                workspace: report.workspace,
                prompt_file: report.prompt_file,
                usage_log: report.usage_summary.log_file.clone(),
                native_invocation: selected.invocation_path,
            },
            usage: report.usage_summary,
            agent_identity: selected.invocation.and_then(|value| value.agent_identity),
        }
    }
}

impl Aggregate {
    fn from_tasks(tasks: &[TaskReport]) -> Result<Self, BenchmarkError> {
        let mut aggregate = Self::default();
        for task in tasks {
            validation::task(task)?;
            aggregate.add(task)?;
        }
        let count = f64::from(u32::try_from(tasks.len()).map_err(|_| {
            BenchmarkError::Workspace("HarnessBench task count exceeds u32".to_owned())
        })?);
        aggregate.outcome_mean /= count;
        aggregate.process_mean /= count;
        aggregate.security_mean /= count;
        aggregate.combined_mean /= count;
        Ok(aggregate)
    }

    fn add(&mut self, task: &TaskReport) -> Result<(), BenchmarkError> {
        self.tasks += 1;
        self.adapter_successes += usize::from(task.adapter_ok);
        self.adapter_failures += usize::from(!task.adapter_ok);
        self.outcome_mean += task.scores.outcome;
        self.process_mean += task.scores.process;
        self.security_mean += task.scores.security;
        self.combined_mean += task.scores.combined;
        self.perfect_outcomes += usize::from(task.scores.outcome >= 0.999_95);
        self.outcomes_at_least_0_9 += usize::from(task.scores.outcome >= 0.9);
        self.combined_at_least_0_9 += usize::from(task.scores.combined >= 0.9);
        self.elapsed_seconds += task.elapsed_seconds;
        self.request_count = checked(self.request_count, task.usage.request_count, "requests")?;
        self.input_tokens = checked(self.input_tokens, task.usage.input_tokens, "input tokens")?;
        self.output_tokens =
            checked(self.output_tokens, task.usage.output_tokens, "output tokens")?;
        self.cache_read_tokens =
            checked(self.cache_read_tokens, task.usage.cache_read_tokens, "cache-read tokens")?;
        self.cache_write_tokens =
            checked(self.cache_write_tokens, task.usage.cache_write_tokens, "cache-write tokens")?;
        self.total_tokens = checked(self.total_tokens, task.usage.total_tokens, "total tokens")?;
        Ok(())
    }
}

fn checked(left: u64, right: u64, label: &str) -> Result<u64, BenchmarkError> {
    left.checked_add(right).ok_or_else(|| {
        BenchmarkError::Workspace(format!("HarnessBench aggregate {label} overflowed"))
    })
}
