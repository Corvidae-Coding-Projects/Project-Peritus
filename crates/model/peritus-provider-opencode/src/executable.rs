//! Explicit discovery and pinning of the official OpenCode executable.

use peritus_provider_core::{ProcessExecutable, ProviderCoreError};
use std::path::Path;

/// Pinned official OpenCode executable, tested with version 1.19.0.
#[derive(Clone, Debug)]
pub struct OpenCodeExecutable(pub(crate) ProcessExecutable);

impl OpenCodeExecutable {
    /// Finds OpenCode on PATH or in the vendor's per-user installation directory.
    ///
    /// # Errors
    /// Returns an actionable installation error if no executable can be pinned.
    pub fn discover() -> Result<Self, ProviderCoreError> {
        for directory in directories() {
            for name in candidate_names() {
                if let Ok(executable) = Self::pin(directory.join(name)) {
                    return Ok(executable);
                }
            }
        }
        Err(unavailable())
    }

    /// Finds a supported executable, skipping incompatible PATH entries.
    ///
    /// Version probes run with isolated configuration and no credentials.
    ///
    /// # Errors
    /// Returns an actionable installation error when no supported candidate is available.
    pub async fn discover_supported(
        cancellation: &peritus_provider_core::CancellationToken,
    ) -> Result<Self, ProviderCoreError> {
        let private = super::isolation::Isolation::metadata()?;
        let mut seen = std::collections::BTreeSet::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
        for directory in directories() {
            for name in candidate_names() {
                let Ok(executable) = Self::pin(directory.join(name)) else { continue };
                if !seen.insert(executable.as_path().to_owned()) {
                    continue;
                }
                if std::time::Instant::now() >= deadline {
                    return Err(unavailable());
                }
                match executable.require_supported(&private, cancellation).await {
                    Ok(()) => return Ok(executable),
                    Err(error)
                        if error.kind()
                            == peritus_provider_core::ProviderCoreErrorKind::Cancelled =>
                    {
                        return Err(error);
                    }
                    Err(_) => {}
                }
            }
        }
        Err(unavailable())
    }

    /// Canonicalizes an explicitly selected regular executable.
    ///
    /// # Errors
    /// Rejects missing, non-regular, or non-executable files.
    pub fn pin(path: impl AsRef<Path>) -> Result<Self, ProviderCoreError> {
        ProcessExecutable::pin(path).map(Self)
    }

    /// Returns its pinned native path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        self.0.as_path()
    }

    pub(crate) async fn require_supported(
        &self,
        private: &super::isolation::Isolation,
        cancellation: &peritus_provider_core::CancellationToken,
    ) -> Result<(), ProviderCoreError> {
        use peritus_provider_core::ProcessTransport as _;
        let limits = peritus_provider_core::ProcessLimits::new(
            1,
            4096,
            4096,
            std::time::Duration::from_secs(10),
        )?;
        let request = private.process(self, vec!["--version".into()], Vec::new(), limits)?;
        let output =
            peritus_provider_core::TokioProcessTransport.run(request, cancellation).await?;
        if !output.exit().success() || output.stdout().trim_ascii() != b"1.19.0" {
            return Err(ProviderCoreError::configuration(
                "opencode_runtime_version",
                "this isolated transport requires official OpenCode 1.19.0; install that version and retry",
            ));
        }
        Ok(())
    }
}

fn directories() -> Vec<std::path::PathBuf> {
    let mut directories = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        directories.push(std::path::PathBuf::from(&home).join(".opencode/bin"));
        directories.push(std::path::PathBuf::from(home).join(".local/bin"));
    }
    directories
}

fn unavailable() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "opencode_runtime_executable",
        "supported OpenCode 1.19.0 executable not found; install the official pinned CLI, then retry provider setup",
    )
}

#[cfg(windows)]
const fn candidate_names() -> &'static [&'static str] {
    &["opencode.exe", "opencode.cmd"]
}
#[cfg(not(windows))]
const fn candidate_names() -> &'static [&'static str] {
    &["opencode"]
}
