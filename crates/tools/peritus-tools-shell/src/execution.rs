//! Owned C2 process to C4 active-execution adapter.

mod active;
pub mod failure;
mod progress;
mod terminal;

pub use active::ShellExecution;
