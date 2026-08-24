//! Discovery and immutable pinning of `OpenAI`'s unmodified `codex` executable.

use core::fmt;
use std::path::Path;

use peritus_provider_core::{ProcessExecutable, ProviderCoreError};

/// Immutable canonical path to `OpenAI`'s credential-owning `Codex` executable.
#[derive(Clone, Eq, PartialEq)]
pub struct CodexExecutable(ProcessExecutable);

impl CodexExecutable {
    /// Finds `codex` on the startup `PATH` and pins its canonical path.
    ///
    /// # Errors
    ///
    /// Returns a redaction-safe configuration error when no regular executable is found.
    pub fn discover() -> Result<Self, ProviderCoreError> {
        let path = std::env::var_os("PATH").ok_or_else(not_installed)?;
        for directory in std::env::split_paths(&path) {
            for name in candidate_names() {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return Self::pin(candidate);
                }
            }
        }
        Err(not_installed())
    }

    /// Canonicalizes and pins one explicit executable path.
    ///
    /// # Errors
    ///
    /// Rejects a missing, non-regular, or non-executable path.
    pub fn pin(path: impl AsRef<Path>) -> Result<Self, ProviderCoreError> {
        ProcessExecutable::pin(path).map(Self)
    }

    /// Borrows the pinned path for explicit user-facing diagnostics.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub(super) const fn process_executable(&self) -> &ProcessExecutable {
        &self.0
    }
}

impl fmt::Debug for CodexExecutable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CodexExecutable([pinned])")
    }
}

#[cfg(windows)]
const fn candidate_names() -> &'static [&'static str] {
    &["codex.exe", "codex", "codex.cmd", "codex.bat"]
}

#[cfg(not(windows))]
const fn candidate_names() -> &'static [&'static str] {
    &["codex"]
}

const fn not_installed() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "codex_runtime_executable",
        "Codex executable not found; install Codex and run `codex login` externally",
    )
}
