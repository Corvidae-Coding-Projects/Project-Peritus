//! First-party stable-v1 Google Gemini adapter for Peritus.
//!
//! The public boundary contains only Peritus protocol/core values. Google wire structures,
//! credentials, payloads, and SSE bytes remain private and redacted from diagnostics.

mod client;
mod config;
mod error;
mod profile;
mod request;
mod stream;

pub use client::GoogleClient;
pub use config::GoogleConfig;
pub use profile::validate_google_profile;
