//! Versioned terminal and retained invocation evidence.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_runner::{ProductRunPhase, ProductRunUpdate};
use serde::Serialize;

use crate::{BenchmarkAgentIdentity, BenchmarkError};

/// Sandbox-relative evidence locations that remain valid if a benchmark runner relocates the
/// completed sandbox after inspecting provider metadata.
#[derive(Clone, Debug, Serialize)]
pub struct RelocatablePaths {
    /// Base directory against which every path in this object resolves.
    pub base: &'static str,
    /// Benchmark workspace below the sandbox.
    pub workspace: PathBuf,
    /// Current durable D0 trace below the sandbox.
    pub trace_path: PathBuf,
    /// Ordered D0 traces below the sandbox.
    pub session_trace_paths: Vec<PathBuf>,
    /// `HarnessBench` usage-proxy directory below the sandbox.
    pub usage_proxy: PathBuf,
    /// Last product observation below the sandbox, when present.
    pub last_observation_path: Option<PathBuf>,
}

/// Machine-readable result from one native benchmark-agent invocation.
#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    /// Evidence schema version.
    pub schema_version: u32,
    /// Exact source, package, and executable identity for this native invocation.
    pub agent_identity: BenchmarkAgentIdentity,
    /// Whether the real Peritus product composition completed successfully.
    pub success: bool,
    /// Upstream benchmark task identity.
    pub task_id: String,
    /// Upstream benchmark session identity.
    pub session_id: String,
    /// Harness configuration identity.
    pub harness_model_id: String,
    /// Canonical benchmark workspace.
    pub workspace: PathBuf,
    /// Exact Git baseline used by Peritus.
    pub baseline_head: String,
    /// Whether the adapter established a new local fixture repository.
    pub initialized_repository: bool,
    /// Whether the adapter declared a new explicit artifact workspace contract.
    pub created_artifact_manifest: bool,
    /// Writer/fixer provider and model.
    pub writer: String,
    /// Independent reviewer provider and model.
    pub reviewer: String,
    /// Elapsed wall-clock milliseconds.
    pub elapsed_ms: u128,
    /// Original durable D0 trace.
    pub trace_path: PathBuf,
    /// One-based durable conversation turn represented by this invocation.
    pub conversation_turn: usize,
    /// Ordered D0 trace paths for every completed session turn.
    pub session_trace_paths: Vec<PathBuf>,
    /// `HarnessBench` usage-proxy directory.
    pub usage_proxy: PathBuf,
    /// Number of projected provider response records.
    pub projected_responses: usize,
    /// Durable exact last product observation when the runner emitted one.
    pub last_observation_path: Option<PathBuf>,
    /// Relocation-safe paths rooted at the final sandbox reported by the benchmark runner.
    pub relocatable_paths: RelocatablePaths,
    /// Product-level completion summary when successful.
    pub summary: Option<String>,
    /// Exact changed paths accepted by the product runner.
    pub changed_paths: Vec<PathBuf>,
    /// Stable product-run failure category when unsuccessful.
    pub failure_kind: Option<String>,
    /// Redaction-safe product-run failure detail when unsuccessful.
    pub failure: Option<String>,
}

/// Report returned by either supported external benchmark protocol.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum BenchmarkReport {
    /// `HarnessBench` invocation evidence.
    HarnessBench(RunReport),
    /// Terminal-Bench invocation evidence.
    TerminalBench(TerminalBenchReport),
}

/// Aggregate provider accounting reconstructed from the native trace.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TraceUsage {
    /// Completed model responses represented by the trace.
    pub requests: usize,
    /// Provider-reported input tokens.
    pub input_tokens: u64,
    /// Provider-reported or observed cached input tokens.
    pub cached_input_tokens: u64,
    /// Provider-reported output tokens.
    pub output_tokens: u64,
    /// Provider-reported totals, or derived input-plus-output totals.
    pub total_tokens: u64,
    /// Provider-reported cost in millionths of the provider currency unit.
    pub provider_cost_microunits: u64,
}

/// Machine-readable result from one Peritus run inside a Harbor task environment.
#[derive(Clone, Debug, Serialize)]
pub struct TerminalBenchReport {
    /// Evidence schema version.
    pub schema_version: u32,
    /// Exact source, package, and executable identity for this native invocation.
    pub agent_identity: BenchmarkAgentIdentity,
    /// Whether the native Peritus product composition accepted the candidate.
    pub success: bool,
    /// Upstream task identity.
    pub task_id: String,
    /// Harbor trial identity.
    pub session_id: String,
    /// Model label recorded by Harbor.
    pub harness_model_id: String,
    /// Canonical task workspace.
    pub workspace: PathBuf,
    /// Exact Git baseline used by Peritus.
    pub baseline_head: String,
    /// Whether Peritus initialized the task workspace as a Git repository.
    pub initialized_repository: bool,
    /// Whether Peritus created its artifact workspace contract.
    pub created_artifact_manifest: bool,
    /// Writer and fixer provider/model identity.
    pub writer: String,
    /// Independent reviewer provider/model identity.
    pub reviewer: String,
    /// Elapsed wall-clock milliseconds.
    pub elapsed_ms: u128,
    /// Durable native provider/tool trace in Harbor's agent logs.
    pub trace_path: PathBuf,
    /// One-based conversation turn represented by this invocation.
    pub conversation_turn: usize,
    /// Aggregate accounting reconstructed from the native trace.
    pub usage: TraceUsage,
    /// Last complete product observation, when one was emitted.
    pub last_observation_path: Option<PathBuf>,
    /// Product completion summary when accepted.
    pub summary: Option<String>,
    /// Exact changed paths accepted by the product runner.
    pub changed_paths: Vec<PathBuf>,
    /// Stable product failure category when not accepted.
    pub failure_kind: Option<String>,
    /// Redaction-safe product failure detail when not accepted.
    pub failure: Option<String>,
}

/// Last daemon-visible product observation, retained outside the benchmark result payload.
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
}

impl ProductObservation {
    pub fn from_update(update: ProductRunUpdate) -> Self {
        Self {
            schema_version: 1,
            phase: phase_name(update.phase),
            cycle: update.cycle,
            status: update.status,
            diff: update.diff,
            gates: update.gates,
            review: update.review,
            summary: update.summary,
            finding_state: update.finding_state,
        }
    }

    pub fn publish(&self, directory: &Path) -> Result<PathBuf, BenchmarkError> {
        publish_json(directory, "last-product-observation.json", self)
    }
}

impl RunReport {
    pub(crate) fn publish(&self, directory: &Path) -> Result<PathBuf, BenchmarkError> {
        publish_json(directory, "invocation.json", self)
    }
}

impl BenchmarkReport {
    /// Returns the product-level acceptance recorded by either report shape.
    #[must_use]
    pub const fn success(&self) -> bool {
        match self {
            Self::HarnessBench(report) => report.success,
            Self::TerminalBench(report) => report.success,
        }
    }
}

impl TerminalBenchReport {
    pub(crate) fn publish(&self, directory: &Path) -> Result<PathBuf, BenchmarkError> {
        publish_json(directory, "invocation.json", self)
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
        .map_err(|error| BenchmarkError::filesystem("write evidence file", &temporary, error))?;
    file.write_all(b"\n")
        .map_err(|error| BenchmarkError::filesystem("finish evidence file", &temporary, error))?;
    file.sync_all()
        .map_err(|error| BenchmarkError::filesystem("sync evidence file", &temporary, error))?;
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
        ProductRunPhase::Complete => "complete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_observation_retains_gate_and_review_diagnostics() {
        let directory = tempfile::tempdir().expect("evidence directory");
        let observation = ProductObservation::from_update(ProductRunUpdate {
            phase: ProductRunPhase::Reviewing,
            cycle: 3,
            status: "Fresh typed review completed".to_owned(),
            diff: "exact diff".to_owned(),
            gates: "Exact-target acceptance: PASS".to_owned(),
            review: "Canonical reason remains contradictory".to_owned(),
            summary: "candidate retained".to_owned(),
            finding_state: "{\"cycle\":3}".to_owned(),
            progress: peritus_product_runner::ProductRunProgress::default(),
        });

        let path = observation.publish(directory.path()).expect("publish observation");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("read observation"))
                .expect("parse observation");

        assert_eq!(value["phase"], "reviewing");
        assert_eq!(value["cycle"], 3);
        assert_eq!(value["diff"], "exact diff");
        assert_eq!(value["gates"], "Exact-target acceptance: PASS");
        assert_eq!(value["review"], "Canonical reason remains contradictory");
        assert_eq!(value["finding_state"], "{\"cycle\":3}");
    }

    #[test]
    fn relocatable_paths_survive_a_sandbox_directory_move() {
        let sandbox = Path::new("/state/workspaces/model/task-before");
        let trace = sandbox.join("peritus-benchmark/developer-round-0001.trace");
        let observation = sandbox.join("peritus-benchmark/last-product-observation.json");
        let paths = RelocatablePaths::new(
            sandbox,
            &sandbox.join("workspace"),
            &trace,
            std::slice::from_ref(&trace),
            &sandbox.join("usage-proxy"),
            Some(&observation),
        )
        .expect("sandbox-relative paths");

        assert_eq!(paths.base, "sandbox");
        assert_eq!(paths.workspace, Path::new("workspace"));
        assert_eq!(paths.trace_path, Path::new("peritus-benchmark/developer-round-0001.trace"));
        assert_eq!(paths.usage_proxy, Path::new("usage-proxy"));
        assert_eq!(
            Path::new("/state/workspaces/model/task-after").join(&paths.trace_path),
            Path::new(
                "/state/workspaces/model/task-after/peritus-benchmark/developer-round-0001.trace"
            )
        );
    }
}
