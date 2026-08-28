//! Versioned terminal and retained invocation evidence.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

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
    /// `HarnessBench` usage-proxy directory.
    pub usage_proxy: PathBuf,
    /// Number of projected provider response records.
    pub projected_responses: usize,
    /// Product-level completion summary when successful.
    pub summary: Option<String>,
    /// Exact changed paths accepted by the product runner.
    pub changed_paths: Vec<PathBuf>,
    /// Stable product-run failure category when unsuccessful.
    pub failure_kind: Option<String>,
    /// Redaction-safe product-run failure detail when unsuccessful.
    pub failure: Option<String>,
}

impl RunReport {
    pub(crate) fn publish(&self, directory: &Path) -> Result<PathBuf, BenchmarkError> {
        fs::create_dir_all(directory).map_err(|error| {
            BenchmarkError::filesystem("create evidence directory", directory, error)
        })?;
        let path = directory.join("invocation.json");
        let temporary = directory.join("invocation.json.new");
        let bytes = serde_json::to_vec_pretty(self)?;
        let mut file = fs::File::create(&temporary).map_err(|error| {
            BenchmarkError::filesystem("create evidence file", &temporary, error)
        })?;
        file.write_all(&bytes).map_err(|error| {
            BenchmarkError::filesystem("write evidence file", &temporary, error)
        })?;
        file.write_all(b"\n").map_err(|error| {
            BenchmarkError::filesystem("finish evidence file", &temporary, error)
        })?;
        file.sync_all()
            .map_err(|error| BenchmarkError::filesystem("sync evidence file", &temporary, error))?;
        fs::rename(&temporary, &path)
            .map_err(|error| BenchmarkError::filesystem("publish evidence file", &path, error))?;
        Ok(path)
    }
}
