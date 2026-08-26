//! Strict schema-v1 manifest parsing and C1-backed inventory loading.

mod document;
mod error;
mod inventory;
mod loader;
mod tags;

pub use document::HarnessManifest;
pub use error::{ManifestError, ManifestErrorKind};
pub use loader::{CheckedLoadedHarness, LoadedHarness, load_harness};
