//! Versioned terminal and retained invocation evidence.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_runner::{ProductRunPhase, ProductRunProgress, ProductRunUpdate};
use serde::Serialize;

use crate::{BenchmarkAgentIdentity, BenchmarkError, candidate::CandidateSnapshot};

/// External suite entering the native adapter boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkSuite {
    /// Qihoo360 Harness-Bench.
    HarnessBench,
    /// Harbor Terminal-Bench 2.
    TerminalBench,
}

/// Pre-run contract retained with the terminal report.
#[derive(Clone, Debug, Serialize)]
pub struct HandshakeReport {
    pub adapter_schema_version: u32,
    pub product_protocol_version: u32,
    pub suite_revision: String,
    pub config_digest: String,
    pub workspace_available: bool,
    pub workspace: PathBuf,
    pub trace_path: PathBuf,
    pub evidence_path: PathBuf,
    pub recovery_path: PathBuf,
    pub agent_identity: BenchmarkAgentIdentity,
    pub provider_routes: Vec<ProviderRouteReport>,
}

/// Declared and live-qualified route for one model role.
#[derive(Clone, Debug, Serialize)]
pub struct ProviderRouteReport {
    pub role: &'static str,
    pub provider: String,
    pub model: String,
    pub route: &'static str,
    pub availability: &'static str,
    pub text: bool,
    pub image_input: bool,
    pub maximum_context_tokens: u64,
    pub tool_protocol: bool,
}

/// Current candidate-bound acceptance evidence.
#[derive(Clone, Debug, Serialize)]
pub struct QualificationReport {
    pub stage: &'static str,
    pub gates: &'static str,
    pub obligations: &'static str,
    pub review: &'static str,
    pub gate_detail: Option<String>,
    pub review_detail: Option<String>,
}

impl QualificationReport {
    pub(crate) const fn missing() -> Self {
        Self {
            stage: "not_observed",
            gates: "missing",
            obligations: "missing",
            review: "missing",
            gate_detail: None,
            review_detail: None,
        }
    }

    pub(crate) const fn candidate(
        stage: &'static str,
        gate_detail: Option<String>,
        review_detail: Option<String>,
    ) -> Self {
        Self {
            stage,
            gates: "incomplete",
            obligations: "incomplete",
            review: "incomplete",
            gate_detail,
            review_detail,
        }
    }

    pub(crate) const fn accepted(gates: String, review: String) -> Self {
        Self {
            stage: "qualified",
            gates: "satisfied",
            obligations: "satisfied",
            review: "satisfied",
            gate_detail: Some(gates),
            review_detail: Some(review),
        }
    }
}

/// Exact candidate details retained independently of benchmark reward.
#[derive(Clone, Debug, Serialize)]
pub struct CandidateReport {
    pub stage: &'static str,
    pub digest: String,
    pub changed_paths: Vec<PathBuf>,
}

impl CandidateReport {
    pub(crate) fn from_snapshot(snapshot: &CandidateSnapshot, stage: &'static str) -> Self {
        Self {
            stage,
            digest: snapshot.digest.clone(),
            changed_paths: snapshot.changed_paths.clone(),
        }
    }
}

/// Benchmark-owned scoring facts. Native settlement never reads these values.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ExternalEvaluation {
    pub reward: Option<f64>,
    pub verifier_exception: Option<String>,
}

/// Bounded resource accounting at the last completed product effect boundary.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct ResourceReport {
    pub model_requests: u32,
    pub tool_calls: u32,
    pub retries: u32,
    pub provider_failovers: u32,
    pub compactions: u32,
    pub elapsed_millis: u64,
    pub workspace_bytes: u64,
    pub workspace_growth_bytes: u64,
    pub peak_rss_bytes: u64,
}

impl From<ProductRunProgress> for ResourceReport {
    fn from(progress: ProductRunProgress) -> Self {
        Self {
            model_requests: progress.model_requests(),
            tool_calls: progress.tool_calls(),
            retries: progress.retries(),
            provider_failovers: progress.provider_failovers(),
            compactions: progress.compactions(),
            elapsed_millis: progress.elapsed_millis(),
            workspace_bytes: progress.workspace_bytes(),
            workspace_growth_bytes: progress.workspace_growth_bytes(),
            peak_rss_bytes: progress.peak_rss_bytes(),
        }
    }
}

/// Aggregate provider accounting reconstructed from the native trace.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TraceUsage {
    /// Completed model responses represented by the trace.
    pub requests: usize,
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported cached input tokens.
    pub cached_input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
    /// Provider-reported or conservatively derived total tokens.
    pub total_tokens: u64,
    /// Provider-reported cost in millionths of its currency unit.
    pub provider_cost_microunits: u64,
}

/// Sandbox-relative evidence locations preserved for Harness-Bench relocation.
#[derive(Clone, Debug, Serialize)]
pub struct RelocatablePaths {
    pub base: &'static str,
    pub workspace: PathBuf,
    pub trace_path: PathBuf,
    pub session_trace_paths: Vec<PathBuf>,
    pub usage_proxy: PathBuf,
    pub last_observation_path: Option<PathBuf>,
}

/// Universal native report emitted by both external adapters.
#[derive(Clone, Debug, Serialize)]
pub struct InvocationReport {
    pub schema_version: u32,
    pub suite: BenchmarkSuite,
    pub handshake: HandshakeReport,
    pub agent_identity: BenchmarkAgentIdentity,
    pub success: bool,
    pub disposition: &'static str,
    pub terminal_cause: &'static str,
    pub candidate: Option<CandidateReport>,
    pub qualification: QualificationReport,
    pub provider_routes: Vec<ProviderRouteReport>,
    pub external_evaluation: ExternalEvaluation,
    pub task_id: String,
    pub session_id: String,
    pub harness_model_id: String,
    pub workspace: PathBuf,
    pub baseline_head: Option<String>,
    pub initialized_repository: bool,
    pub created_artifact_manifest: bool,
    pub writer: String,
    pub reviewer: String,
    pub elapsed_ms: u128,
    pub trace_path: PathBuf,
    pub conversation_turn: usize,
    pub session_trace_paths: Vec<PathBuf>,
    pub usage_proxy: Option<PathBuf>,
    pub projected_responses: usize,
    pub usage: TraceUsage,
    pub resources: ResourceReport,
    pub last_observation_path: Option<PathBuf>,
    pub relocatable_paths: Option<RelocatablePaths>,
    pub summary: Option<String>,
    pub changed_paths: Vec<PathBuf>,
    pub failure_kind: Option<String>,
    pub failure: Option<String>,
}

/// Harness-Bench name for the universal invocation report.
pub type RunReport = InvocationReport;
/// Terminal-Bench name for the universal invocation report.
pub type TerminalBenchReport = InvocationReport;

/// Report returned by either supported external benchmark protocol.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum BenchmarkReport {
    /// Harness-Bench invocation evidence.
    HarnessBench(RunReport),
    /// Terminal-Bench invocation evidence.
    TerminalBench(TerminalBenchReport),
}

impl BenchmarkReport {
    /// Whether strict native settlement accepted the exact candidate.
    #[must_use]
    pub const fn success(&self) -> bool {
        match self {
            Self::HarnessBench(report) | Self::TerminalBench(report) => report.success,
        }
    }
}

/// Last daemon-visible product observation retained outside the result payload.
#[derive(Clone, Debug, Serialize)]
pub struct ProductObservation {
    schema_version: u32,
    phase: &'static str,
    cycle: u32,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
    finding_state: String,
    resources: ResourceReport,
}

impl ProductObservation {
    pub fn from_update(update: ProductRunUpdate) -> Self {
        Self {
            schema_version: 2,
            phase: phase_name(update.phase),
            cycle: update.cycle,
            status: update.status,
            diff: update.diff,
            gates: update.gates,
            review: update.review,
            summary: update.summary,
            finding_state: update.finding_state,
            resources: update.progress.into(),
        }
    }

    pub fn publish(&self, directory: &Path) -> Result<PathBuf, BenchmarkError> {
        publish_json(directory, "last-product-observation.json", self)
    }
}

impl RelocatablePaths {
    pub(crate) fn new(
        sandbox: &Path,
        workspace: &Path,
        trace_path: &Path,
        session_trace_paths: &[PathBuf],
        usage_proxy: &Path,
        last_observation_path: Option<&Path>,
    ) -> Result<Self, BenchmarkError> {
        Ok(Self {
            base: "sandbox",
            workspace: below(sandbox, workspace)?,
            trace_path: below(sandbox, trace_path)?,
            session_trace_paths: session_trace_paths
                .iter()
                .map(|path| below(sandbox, path))
                .collect::<Result<Vec<_>, _>>()?,
            usage_proxy: below(sandbox, usage_proxy)?,
            last_observation_path: last_observation_path
                .map(|path| below(sandbox, path))
                .transpose()?,
        })
    }
}

fn below(sandbox: &Path, path: &Path) -> Result<PathBuf, BenchmarkError> {
    path.strip_prefix(sandbox).map(Path::to_path_buf).map_err(|_| {
        BenchmarkError::Workspace(
            "retained benchmark evidence path is outside its owned sandbox".to_owned(),
        )
    })
}

fn publish_json(
    directory: &Path,
    name: &str,
    value: &(impl Serialize + ?Sized),
) -> Result<PathBuf, BenchmarkError> {
    fs::create_dir_all(directory).map_err(|error| {
        BenchmarkError::filesystem("create evidence directory", directory, error)
    })?;
    let path = directory.join(name);
    let temporary = directory.join(format!("{name}.new"));
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = fs::File::create(&temporary)
        .map_err(|error| BenchmarkError::filesystem("create evidence file", &temporary, error))?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| BenchmarkError::filesystem("write evidence file", &temporary, error))?;
    fs::rename(&temporary, &path)
        .map_err(|error| BenchmarkError::filesystem("publish evidence file", &path, error))?;
    Ok(path)
}

const fn phase_name(phase: ProductRunPhase) -> &'static str {
    match phase {
        ProductRunPhase::Designing => "designing",
        ProductRunPhase::Writing => "writing",
        ProductRunPhase::Checking => "checking",
        ProductRunPhase::Reviewing => "reviewing",
        ProductRunPhase::Fixing => "fixing",
        ProductRunPhase::Verifying => "verifying",
        ProductRunPhase::Finalizing => "finalizing",
        ProductRunPhase::Complete => "complete",
    }
}

#[cfg(test)]
mod tests;
