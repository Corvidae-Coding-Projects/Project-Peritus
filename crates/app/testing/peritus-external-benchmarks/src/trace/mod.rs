//! Projection of the durable D0 trace into external benchmark evidence.

mod bounded;
mod frames;
mod harnessbench;
mod metadata;
mod projection;

use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use crate::{BenchmarkError, evidence::TraceUsage};

pub struct HarnessTraceEvidence {
    pub projected_responses: usize,
    pub usage: TraceUsage,
    pub incomplete_response: Option<PathBuf>,
}

pub fn prepare(trace_path: &Path) -> Result<(), BenchmarkError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_path)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            BenchmarkError::trace(trace_path, format!("prepare durable developer trace: {error}"))
        })
}

pub fn publish_harnessbench(
    trace_inputs: &[(PathBuf, String)],
    proxy_dir: &Path,
    task_id: &str,
    session_id: &str,
    model_id: &str,
) -> Result<HarnessTraceEvidence, BenchmarkError> {
    let mut rounds = Vec::new();
    let mut incomplete_response = None;
    for (trace_path, initial_user_prompt) in trace_inputs {
        let frames = frames::read(trace_path)?;
        let projected = projection::project(trace_path, &frames, initial_user_prompt)?;
        if projected.incomplete_response && incomplete_response.is_none() {
            incomplete_response = Some(trace_path.clone());
        }
        rounds.extend(projected.rounds);
    }
    let usage = summarize_rounds(&rounds);
    let projected_responses =
        harnessbench::publish(proxy_dir, task_id, session_id, model_id, &rounds)?;
    Ok(HarnessTraceEvidence { projected_responses, usage, incomplete_response })
}

pub fn summarize_usage(
    trace_path: &Path,
    initial_user_prompt: &str,
) -> Result<TraceUsage, BenchmarkError> {
    let frames = frames::read(trace_path)?;
    let projected = projection::project(trace_path, &frames, initial_user_prompt)?;
    if projected.incomplete_response {
        return Err(BenchmarkError::trace(trace_path, "provider response has no terminal event"));
    }
    Ok(summarize_rounds(&projected.rounds))
}

fn summarize_rounds(rounds: &[projection::Round]) -> TraceUsage {
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
    aggregate
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn empty_prepared_trace_has_zero_usage() {
        let directory = tempfile::tempdir().expect("trace directory");
        let path = directory.path().join("empty.trace");
        prepare(&path).expect("prepare trace");

        let usage = summarize_usage(&path, "task").expect("summarize empty trace");

        assert_eq!(usage.requests, 0);
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.provider_cost_microunits, 0);
    }

    #[test]
    fn missing_trace_has_a_distinct_stable_failure_kind() {
        let directory = tempfile::tempdir().expect("trace directory");
        let path = directory.path().join("missing.trace");
        let error = summarize_usage(&path, "task").expect_err("missing trace");
        assert_eq!(error.stable_kind(), "trace");
    }

    #[test]
    fn unwritable_trace_target_has_a_distinct_stable_failure_kind() {
        let directory = tempfile::tempdir().expect("trace directory");
        let path = directory.path().join("trace-is-a-directory");
        fs::create_dir(&path).expect("blocking directory");

        let error = prepare(&path).expect_err("directory cannot be opened as a trace file");

        assert_eq!(error.stable_kind(), "trace");
    }

    #[test]
    fn preparing_an_existing_trace_never_truncates_it() {
        let directory = tempfile::tempdir().expect("trace directory");
        let path = directory.path().join("existing.trace");
        fs::write(&path, b"retained").expect("existing trace");

        prepare(&path).expect("prepare existing trace");

        assert_eq!(fs::read(path).expect("read trace"), b"retained");
    }
}
