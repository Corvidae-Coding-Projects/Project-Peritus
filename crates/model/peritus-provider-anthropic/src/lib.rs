//! First-party Anthropic Messages adapter for Peritus.
//!
//! The public boundary contains only Peritus protocol/core values. Anthropic wire structures,
//! credentials, payloads, and SSE bytes remain private and redacted from diagnostics.

mod client;
mod config;
mod error;
mod profile;
mod request;
mod runtime;
mod stream;
mod wire;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod conformance_tests;

#[cfg(test)]
mod fixture_tests;

pub use client::AnthropicClient;
pub use config::{AnthropicBeta, AnthropicConfig};
pub use profile::validate_anthropic_profile;
pub use runtime::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
