//! Projection of the durable D0 trace into external benchmark evidence.

mod bounded;
mod frames;
mod harnessbench;
mod projection;

use std::path::{Path, PathBuf};

use crate::{BenchmarkError, evidence::TraceUsage};

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

pub fn summarize_usage(
    trace_path: &Path,
    initial_user_prompt: &str,
) -> Result<TraceUsage, BenchmarkError> {
    let frames = frames::read(trace_path)?;
    let rounds = projection::project(trace_path, &frames, initial_user_prompt)?;
    let mut aggregate = TraceUsage { requests: rounds.len(), ..TraceUsage::default() };
    for round in rounds {
        let input = round.usage.input_tokens().unwrap_or(0);
        let output = round.usage.output_tokens().unwrap_or(0);
        aggregate.input_tokens = aggregate.input_tokens.saturating_add(input);
        aggregate.cached_input_tokens = aggregate.cached_input_tokens.saturating_add(
            round.usage.cached_input_tokens().or(round.observed_cache_tokens).unwrap_or(0),
        );
        aggregate.output_tokens = aggregate.output_tokens.saturating_add(output);
        aggregate.total_tokens = aggregate.total_tokens.saturating_add(
            round.usage.total_tokens().unwrap_or_else(|| input.saturating_add(output)),
        );
        aggregate.provider_cost_microunits = aggregate
            .provider_cost_microunits
            .saturating_add(round.usage.provider_cost_microunits().unwrap_or(0));
    }
    Ok(aggregate)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn empty_prepared_trace_has_zero_usage() {
        let directory = tempfile::tempdir().expect("trace directory");
        let path = directory.path().join("empty.trace");
        fs::write(&path, []).expect("empty trace");

        let usage = summarize_usage(&path, "task").expect("summarize empty trace");

        assert_eq!(usage.requests, 0);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.provider_cost_microunits, 0);
    }
}
