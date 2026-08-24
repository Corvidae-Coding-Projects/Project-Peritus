//! Explicitly profiled compatible model-provider adapters for Peritus.
//!
//! Every instance binds a caller-supplied exact endpoint to one separately validated Responses or
//! Chat Completions contract. Compatible behavior is never inferred from URL shape or discovery.

mod client;
mod config;
mod error;
mod profile;
mod request;
mod stream;

pub use client::CompatibleClient;
pub use config::{
    CompatibleAuth, CompatibleConfig, CompatibleHeader, CompatibleRateHeaders, CompatibleResetUnit,
    CompatibleResponseHeaders, CompatibleRetryStatuses, CredentialScheme,
};
pub use profile::{
    CompatibleContract, CompatibleProfile, CreateReplayGuarantee, EventMapping, RequestField,
    ResponseIdSemantics, StreamFraming,
};

#[cfg(test)]
mod tests;
