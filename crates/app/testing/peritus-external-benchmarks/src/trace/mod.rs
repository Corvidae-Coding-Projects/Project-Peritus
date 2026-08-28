//! Projection of the durable D0 trace into external benchmark evidence.

mod frames;
mod harnessbench;
mod projection;

use std::path::{Path, PathBuf};

use crate::BenchmarkError;

pub fn publish_harnessbench(
    trace_inputs: &[(PathBuf, String)],
    proxy_dir: &Path,
    task_id: &str,
    session_id: &str,
    model_id: &str,
) -> Result<usize, BenchmarkError> {
    let mut rounds = Vec::new();
    for (trace_path, initial_user_prompt) in trace_inputs {
        let frames = frames::read(trace_path)?;
        rounds.extend(projection::project(trace_path, &frames, initial_user_prompt)?);
    }
    harnessbench::publish(proxy_dir, task_id, session_id, model_id, &rounds)
}
