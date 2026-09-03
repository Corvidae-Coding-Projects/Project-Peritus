//! Native, noninteractive external-benchmark entry point for Peritus.

mod admission;
mod agent;
mod args;
mod candidate;
mod command_runtime;
mod deadline;
mod dispatch;
mod error;
mod evidence;
pub mod harness_results;
mod identity;
mod process_entry;
mod providers;
mod publication;
mod report_path;
mod rubric;
mod session;
mod settlement;
mod terminal_agent;
pub mod terminal_results;
mod trace;
mod workspace;

pub use dispatch::{complete_rubric, run};
pub use error::BenchmarkError;
pub use evidence::{BenchmarkReport, RunReport, TerminalBenchReport, TraceUsage};
pub use identity::BenchmarkAgentIdentity;
pub use process_entry::main_entry;
