//! First-party `OpenAI` Responses adapter for Peritus.
//!
//! The public boundary contains only validated Peritus protocol and provider-core values. `OpenAI`
//! request/response shapes remain private implementation details.

mod client;
mod config;
mod error;
mod profile;
mod request;
mod runtime;
mod stream;

pub use client::{OpenAiClient, OpenAiProvider};
pub use config::OpenAiConfig;
pub use runtime::{CodexExecutable, CodexRuntimeClient, CodexRuntimeConfig, CodexRuntimeProvider};

#[cfg(test)]
mod tests;
