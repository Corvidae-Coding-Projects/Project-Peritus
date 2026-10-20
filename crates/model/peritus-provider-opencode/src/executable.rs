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

    /// Canonicalizes and pins an explicitly selected native host executable.
    ///
    /// # Errors
    /// Rejects missing, non-regular, non-executable, or foreign-host files.
    pub fn pin(path: impl AsRef<Path>) -> Result<Self, ProviderCoreError> {
        let executable = ProcessExecutable::pin(path)?;
        if !native_program(executable.as_path()) {
            return Err(foreign_program());
        }
        Ok(Self(executable))
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
        "supported OpenCode 1.19.0 executable not found; install the official pinned CLI for this system, then retry provider setup",
    )
}

const fn foreign_program() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "opencode_runtime_executable",
        "this OpenCode is a Windows program reached through WSL interop; install the official OpenCode 1.19.0 CLI for this system, then retry provider setup",
    )
}

/// Whether a pinned candidate is a native program for this host.
///
/// WSL executes Windows programs from the drives it mounts for the distribution. Such a
/// program can answer a version probe while being unable to honour the private POSIX
/// configuration, state, and temporary paths this transport requires, so it is never a
/// supported runtime.
#[cfg(windows)]
const fn native_program(_path: &Path) -> bool {
    true
}

#[cfg(not(windows))]
fn native_program(path: &Path) -> bool {
    !windows_program(path) && !interop_mount(path)
}

/// Whether the file begins with the Windows portable-executable header.
#[cfg(not(windows))]
fn windows_program(path: &Path) -> bool {
    use std::io::Read as _;
    let mut header = [0_u8; 2];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|()| header == *b"MZ")
}

/// Whether the path is reached through a WSL distribution's Windows-drive mount.
///
/// Windows drives use a filesystem type that no native Linux installation uses, and a
/// customized automount root is still identified through its own mount entry.
#[cfg(not(windows))]
fn interop_mount(path: &Path) -> bool {
    if !wsl_kernel() {
        return false;
    }
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let mut containing: Option<(&str, &str)> = None;
    for line in mounts.lines() {
        let mut fields = line.split(' ');
        let (Some(_source), Some(point), Some(kind)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if path.starts_with(point) && containing.is_none_or(|(best, _)| point.len() >= best.len()) {
            containing = Some((point, kind));
        }
    }
    containing.is_some_and(|(_, kind)| matches!(kind, "drvfs" | "9p" | "virtiofs"))
}

/// Whether this Linux host runs a WSL distribution kernel.
#[cfg(not(windows))]
fn wsl_kernel() -> bool {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"))
}

#[cfg(windows)]
const fn candidate_names() -> &'static [&'static str] {
    &["opencode.exe", "opencode.cmd"]
}
#[cfg(not(windows))]
const fn candidate_names() -> &'static [&'static str] {
    &["opencode"]
}
