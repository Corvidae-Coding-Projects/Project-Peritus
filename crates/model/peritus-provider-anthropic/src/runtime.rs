//! Constrained account-backed transport through Anthropic's credential-owning executable.

mod config;
mod executable;
mod output;
mod provider;
mod request;
mod stream;

pub use config::ClaudeRuntimeConfig;
pub use executable::ClaudeExecutable;
pub use provider::ClaudeRuntimeProvider;

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
