//! Authority-owned upload/download transfer services.

mod client;
mod publication;
mod service;

pub use client::ArtifactClient;
pub use service::{ArtifactAuthority, ArtifactPoll};
