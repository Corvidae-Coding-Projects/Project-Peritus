//! Validated native Rust plugin and declarative workflow packages, discovery and durable state.
//! Native execution is owned by the product bridge; host-service permissions remain live.
mod definition;
mod jobs;
mod market;
#[cfg(target_os = "linux")]
pub mod preparation;
mod registry;
#[cfg(target_os = "linux")]
mod source_build;

pub use definition::{Code, Definition, Manifest, Role, Stage};
pub use jobs::{Job, JobState, StageOutput};
pub use market::{
    Catalog, MARKET_REPOSITORY, MarketEntry, ModuleOperation, SourceCatalog, SourceEntry,
};
pub use registry::{Registry, data_root};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, fs, io::Write as _, path::Path};

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
    /// Catalog or pinned package download failed.
    #[error("module market: {0}")]
    Http(#[from] reqwest::Error),
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
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
