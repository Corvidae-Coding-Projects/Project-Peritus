//! Typed release-operator errors.

use std::{io, path::PathBuf};

use peritus_release_artifacts::ArtifactError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OperatorError {
    #[error(
        "invalid invocation: {0}; usage: peritus-release-operator generate <package-record> | retain-and-upload <package-record> <provenance-bundle> <sbom-bundle>"
    )]
    Argument(String),
    #[error("{operation} `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid release metadata: {0}")]
    Metadata(String),
    #[error("release artifact contract failed: {0}")]
    Artifact(#[from] ArtifactError),
    #[error("{operation} failed with status {status}")]
    Command { operation: &'static str, status: std::process::ExitStatus },
    #[error("serialize release evidence: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse release configuration: {0}")]
    Toml(#[from] toml::de::Error),
}

impl OperatorError {
    pub fn argument(detail: impl Into<String>) -> Self {
        Self::Argument(detail.into())
    }

    pub fn usage() -> Self {
        Self::argument("unsupported arguments")
    }

    pub fn metadata(detail: impl Into<String>) -> Self {
        Self::Metadata(detail.into())
    }

    pub fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { operation, path: path.into(), source }
    }
}
