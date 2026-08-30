//! Native, noninteractive external-benchmark entry point for Peritus.

mod agent;
mod args;
mod dispatch;
mod error;
mod evidence;
mod identity;
mod process_entry;
mod providers;
mod rubric;
mod session;
mod terminal_agent;
pub mod terminal_results;
mod trace;
mod workspace;

pub use dispatch::{complete_rubric, run};
pub use error::BenchmarkError;
pub use evidence::{BenchmarkReport, RunReport, TerminalBenchReport, TraceUsage};
pub use identity::BenchmarkAgentIdentity;
pub use process_entry::main_entry;
