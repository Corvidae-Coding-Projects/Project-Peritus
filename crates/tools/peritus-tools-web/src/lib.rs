//! Provider-neutral native web access. Call blocking operations outside an async executor thread.

mod extract;
mod http;
mod search;
mod setup;
mod source;

use std::{env, path::PathBuf};

pub use setup::{configure, setup_local, status, stop};
pub use source::Store;

/// A failed web operation; no unsuccessful response is represented as a read source.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid input or actionable service failure.
    #[error("{0}")]
    Message(String),
    /// Local storage operation failed.
    #[error("web storage: {0}")]
    Io(#[from] std::io::Error),
    /// Network request failed.
    #[error("web request: {0}")]
    Http(#[from] reqwest::Error),
    /// Invalid persisted or upstream JSON.
    #[error("web JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    Error::Message(message.into())
}

/// Per-user data directory; contains no provider credentials or user browser profiles.
///
/// # Errors
///
/// Returns an error when no per-user data directory can be resolved.
pub fn data_root() -> Result<PathBuf, Error> {
    let root = if cfg!(windows) {
        env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Peritus"))
    } else if cfg!(target_os = "macos") {
        env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support/Peritus"))
    } else {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("peritus"))
    };
    root.map(|p| p.join("web")).ok_or_else(|| invalid("user data directory is unavailable"))
}

/// Accept only HTTP(S) URLs without embedded credentials, including on redirects.
///
/// # Errors
///
/// Returns an error when the value is not an absolute credential-free HTTP(S) URL.
pub fn checked_url(value: &str) -> Result<url::Url, Error> {
    let mut url =
        url::Url::parse(value).map_err(|_| invalid("use an absolute HTTP or HTTPS URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid("only HTTP(S) without embedded credentials is supported"));
    }
    url.set_fragment(None);
    Ok(url)
}
