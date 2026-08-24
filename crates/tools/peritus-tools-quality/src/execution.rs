//! Owned quality process projection into the C4 active-execution protocol.

mod active;
pub mod failure;
mod progress;
mod terminal;

pub use active::QualityExecution;
