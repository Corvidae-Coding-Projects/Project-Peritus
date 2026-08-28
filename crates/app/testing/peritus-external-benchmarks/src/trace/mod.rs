//! Projection of the durable D0 trace into external benchmark evidence.

mod frames;
mod harnessbench;
mod projection;

use std::path::Path;

use crate::BenchmarkError;

pub fn publish_harnessbench(
    trace_path: &Path,
    proxy_dir: &Path,
    task_id: &str,
    session_id: &str,
    model_id: &str,
    initial_user_prompt: &str,
) -> Result<usize, BenchmarkError> {
    let frames = frames::read(trace_path)?;
    let rounds = projection::project(trace_path, &frames, initial_user_prompt)?;
    harnessbench::publish(proxy_dir, task_id, session_id, model_id, &rounds)
}
