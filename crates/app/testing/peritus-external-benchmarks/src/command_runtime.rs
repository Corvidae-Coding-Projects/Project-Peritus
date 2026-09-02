//! Protected command-runtime composition shared by external benchmark adapters.

use std::path::Path;

use peritus_process::ProcessStore;
use peritus_product_runner::CommandRuntime;
use peritus_types::RunId;

use crate::BenchmarkError;

pub fn open(workspace: &Path, run_id: RunId) -> Result<CommandRuntime, BenchmarkError> {
    let state =
        std::env::temp_dir().join("peritus-benchmark-command-runtime").join(run_hex(run_id));
    let processes = ProcessStore::open(state.join("processes"), workspace)
        .map_err(|error| BenchmarkError::Workspace(error.to_string()))?;
    CommandRuntime::open(state.join("router"), workspace, run_id, processes)
        .map_err(|error| BenchmarkError::Workspace(error.to_string()))
}

fn run_hex(run_id: RunId) -> String {
    let mut output = String::with_capacity(32);
    for byte in run_id.as_bytes() {
        use core::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
