//! Constrained account-backed transport through `OpenAI`'s credential-owning `Codex` executable.

mod config;
mod executable;
mod live;
pub mod output;
mod provider;
pub mod request;
mod stream;

pub use config::CodexRuntimeConfig;
pub use executable::CodexExecutable;
pub use provider::{CodexRuntimeClient, CodexRuntimeProvider};
