//! Authority-owned upload/download transfer services.

mod client;
mod publication;
mod service;

pub(crate) use client::ArtifactClient;
pub(crate) use service::{ArtifactAuthority, ArtifactPoll};
