//! Versioned terminal and retained invocation evidence.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_runner::{ProductRunPhase, ProductRunUpdate};
use serde::Serialize;

use crate::BenchmarkError;

/// Machine-readable result from one native benchmark-agent invocation.
#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    /// Evidence schema version.
    pub schema_version: u32,
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
    /// Product-level completion summary when successful.
    pub summary: Option<String>,
    /// Exact changed paths accepted by the product runner.
    pub changed_paths: Vec<PathBuf>,
    /// Stable product-run failure category when unsuccessful.
    pub failure_kind: Option<String>,
    /// Redaction-safe product-run failure detail when unsuccessful.
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
}
