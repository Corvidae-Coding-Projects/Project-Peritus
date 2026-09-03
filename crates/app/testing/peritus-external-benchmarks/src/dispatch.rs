//! Typed dispatch from external benchmark protocols into native Peritus responsibilities.

use std::ffi::OsString;

use crate::{BenchmarkError, BenchmarkReport, agent, args, rubric, terminal_agent};

/// Completes one OpenAI-compatible rubric request through the authenticated official `codex`
/// executable and returns a Chat Completions-shaped response.
///
/// # Errors
///
/// Returns a typed failure when the request is oversized, malformed, multimodal, unsupported by
/// the account router, or the provider response is not a successful text completion.
pub async fn complete_rubric(body: &[u8]) -> Result<serde_json::Value, BenchmarkError> {
    rubric::complete(body).await
}

/// Parses one benchmark-agent invocation and runs the real Peritus product composition.
///
/// # Errors
///
/// Returns a typed failure when the command is invalid or the benchmark boundary cannot prepare,
/// execute, trace, or record the run.
pub async fn run<I>(arguments: I) -> Result<BenchmarkReport, BenchmarkError>
where
    I: IntoIterator<Item = OsString>,
{
    let command = args::Command::parse(arguments)?;
    match command {
        args::Command::HarnessBench(input) => {
            Box::pin(agent::run_harnessbench(input)).await.map(BenchmarkReport::HarnessBench)
        }
        args::Command::TerminalBench(input) => {
            Box::pin(terminal_agent::run(input)).await.map(BenchmarkReport::TerminalBench)
        }
    }
}
