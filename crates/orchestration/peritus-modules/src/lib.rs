//! Declarative agent modules. Packages cannot execute code or grant host authority.
mod definition;
mod jobs;
mod registry;

pub use definition::{Definition, Manifest, Role, Stage};
pub use jobs::{Job, JobState, StageOutput};
pub use registry::{Registry, data_root};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Path};

/// A module operation that could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid module/configuration or recoverable lifecycle state.
    #[error("{0}")]
    Invalid(String),
    /// Local state could not be read or published.
    #[error("module storage: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid structured state.
    #[error("module JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Invalid manifest.
    #[error("module manifest: {0}")]
    Toml(#[from] toml::de::Error),
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
fn publish(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let parent = path.parent().ok_or_else(|| invalid("state path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
