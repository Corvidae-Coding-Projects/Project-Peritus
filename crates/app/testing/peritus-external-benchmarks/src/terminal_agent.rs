//! Terminal-Bench admission wrapper around the shared native product composition.

use crate::{
    BenchmarkError, admission, agent, args::TerminalBenchInput, evidence::TerminalBenchReport,
};

pub async fn run(input: TerminalBenchInput) -> Result<TerminalBenchReport, BenchmarkError> {
    agent::execute(admission::terminalbench(input)?).await
}
