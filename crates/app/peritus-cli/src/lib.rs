//! Scriptable command-line client for the local Peritus application protocol.

pub(crate) mod args;
pub(crate) mod artifact;
pub(crate) mod client;
pub(crate) mod completion;
pub(crate) mod error;
pub(crate) mod events;
pub(crate) mod id;
pub(crate) mod operation;
pub(crate) mod output;
pub(crate) mod prompt;
mod runner;
pub(crate) mod terminal;

pub use runner::run_env;
