//! Native, noninteractive external-benchmark entry point for Peritus.

mod agent;
mod args;
mod dispatch;
mod error;
mod evidence;
mod process_entry;
mod providers;
mod rubric;
mod trace;
mod workspace;

pub use dispatch::{complete_rubric, run};
pub use error::BenchmarkError;
pub use evidence::RunReport;
pub use process_entry::main_entry;
