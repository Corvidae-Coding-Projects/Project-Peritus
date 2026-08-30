//! Lexical Windows path normalization and native reparse evidence.

#[cfg(any(target_os = "windows", test))]
use std::path::Path;
use std::path::PathBuf;

use peritus_sandbox::SandboxPath;
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

const MAX_PATH_UNITS: usize = 32_767;

/// Canonical drive-absolute Windows path using `/` separators and an uppercase drive.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowsPath {
    canonical: String,
    case_folded: String,
    digest: Sha256Digest,
}

impl WindowsPath {
    /// Normalizes a drive-absolute Windows path and rejects device/UNC/ADS/reserved syntax.
    ///
    /// # Errors
    /// Rejects non-drive-absolute, traversal, device, UNC, ADS, reserved-name, trailing-dot/space,
    /// wildcard, control, or over-limit representations.
    pub fn new(value: impl AsRef<str>) -> Result<Self, WindowsError> {
        let mut canonical = value.as_ref().replace('\\', "/");
        if canonical.encode_utf16().count() > MAX_PATH_UNITS
            || canonical.starts_with("//")
            || canonical.starts_with("/??/")
            || canonical.len() < 3
            || !canonical.as_bytes()[0].is_ascii_alphabetic()
            || canonical.as_bytes()[1] != b':'
            || canonical.as_bytes()[2] != b'/'
        {
            return Err(path_error("path is not a bounded drive-absolute DOS path"));
        }
        canonical.replace_range(0..1, &canonical[..1].to_ascii_uppercase());
        while canonical.len() > 3 && canonical.ends_with('/') {
            canonical.pop();
        }
        let mut normalized = canonical[..3].to_owned();
        if canonical.len() > 3 {
            for component in canonical[3..].split('/') {
                validate_component(component)?;
                if normalized.len() > 3 {
                    normalized.push('/');
                }
                normalized.push_str(component);
            }
        }
        let case_folded = normalized.to_ascii_lowercase();
        let digest = peritus_codec::sha256(case_folded.as_bytes());
        Ok(Self { canonical: normalized, case_folded, digest })
    }

    /// Converts the trusted drive path returned by `std::fs::canonicalize` into the policy form.
    ///
    /// Windows canonicalization adds the extended-length `\\?\` prefix. That prefix is an OS
    /// representation detail, so native probes remove it before applying the ordinary strict path
    /// policy. UNC and other device paths remain rejected by [`Self::new`].
    #[cfg(any(target_os = "windows", test))]
    pub(crate) fn from_canonicalized(path: &Path) -> Result<Self, WindowsError> {
        let text = path.to_string_lossy();
        Self::new(text.strip_prefix(r"\\?\").unwrap_or(&text))
    }

    /// Resolves a logical sandbox path beneath a canonical workspace.
    pub(crate) fn from_sandbox(workspace: &Self, path: &SandboxPath) -> Result<Self, WindowsError> {
        let text = path.as_str();
        if text.len() >= 3 && text.as_bytes()[1] == b':' {
            return Self::new(text);
        }
        let relative =
            text.strip_prefix('/').ok_or_else(|| path_error("logical path is not absolute"))?;
        let joined = if relative.is_empty() {
            workspace.canonical.clone()
        } else {
            format!("{}/{relative}", workspace.canonical)
        };
        Self::new(joined)
    }

    /// Returns canonical DOS path text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Returns deterministic ASCII case-folded identity.
    #[must_use]
    pub fn case_folded(&self) -> &str {
        &self.case_folded
    }

    /// Returns the normalized case-folded path digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports exact same-volume containment using component boundaries.
    #[must_use]
    pub fn contains(&self, candidate: &Self) -> bool {
        self.case_folded == candidate.case_folded
            || (candidate.case_folded.starts_with(&self.case_folded)
                && candidate.case_folded.as_bytes().get(self.case_folded.len()) == Some(&b'/'))
    }

    /// Returns an OS path using the platform-native separator parser.
    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.canonical.replace('/', "\\"))
    }
}

/// Native path-resolution evidence used by preparation and recovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathEvidence {
    lexical: WindowsPath,
    resolved: WindowsPath,
    volume_serial: u64,
    reparse_free: bool,
    exists: bool,
}

impl PathEvidence {
    /// Creates checked fixture/native evidence.
    ///
    /// # Errors
    /// Rejects zero volume identity or a case/volume escape.
    pub fn new(
        lexical: WindowsPath,
        resolved: WindowsPath,
        volume_serial: u64,
        reparse_free: bool,
        exists: bool,
    ) -> Result<Self, WindowsError> {
        if volume_serial == 0 || lexical.case_folded() != resolved.case_folded() {
            return Err(path_error("resolved path identity differs from its authorized path"));
        }
        Ok(Self { lexical, resolved, volume_serial, reparse_free, exists })
    }

    /// Returns lexical identity.
    #[must_use]
    pub const fn lexical(&self) -> &WindowsPath {
        &self.lexical
    }

    /// Returns resolved identity.
    #[must_use]
    pub const fn resolved(&self) -> &WindowsPath {
        &self.resolved
    }

    /// Returns volume identity.
    #[must_use]
    pub const fn volume_serial(&self) -> u64 {
        self.volume_serial
    }

    /// Reports absence of reparse points in the traversed existing prefix.
    #[must_use]
    pub const fn reparse_free(&self) -> bool {
        self.reparse_free
    }

    /// Reports whether the exact final entry exists.
    #[must_use]
    pub const fn exists(&self) -> bool {
        self.exists
    }
}

/// A path accepted for native ACL use after exact reparse/volume validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedWindowsPath(PathEvidence);

impl ResolvedWindowsPath {
    /// Validates supplied path evidence.
    ///
    /// # Errors
    /// Rejects reparse traversal or a missing final object.
    pub fn from_evidence(evidence: PathEvidence) -> Result<Self, WindowsError> {
        if !evidence.reparse_free || !evidence.exists {
            return Err(path_error("path is missing or traverses a reparse point"));
        }
        Ok(Self(evidence))
    }

    /// Resolves and checks every existing component on Windows.
    ///
    /// # Errors
    /// Rejects inaccessible, missing, reparse-bearing, or canonicalization-changing paths.
    #[cfg(target_os = "windows")]
    pub fn resolve(path: WindowsPath) -> Result<Self, WindowsError> {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        let native = path.to_path_buf();
        let mut current = Some(native.as_path());
        while let Some(candidate) = current {
            let metadata = std::fs::symlink_metadata(candidate)
                .map_err(|_| path_error("path component cannot be inspected"))?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(path_error("path traverses a Windows reparse point"));
            }
            current = candidate.parent();
        }
        let canonical = std::fs::canonicalize(&native)
            .map_err(|_| path_error("path cannot be resolved exactly"))?;
        let resolved_text = canonical.to_string_lossy();
        let resolved_text = resolved_text.strip_prefix(r"\\?\").unwrap_or(&resolved_text);
        let resolved = WindowsPath::new(resolved_text)?;
        let evidence = PathEvidence::new(path, resolved, volume_serial(&native)?, true, true)?;
        Self::from_evidence(evidence)
    }

    /// Returns the checked evidence.
    #[must_use]
    pub const fn evidence(&self) -> &PathEvidence {
        &self.0
    }
}

#[cfg(target_os = "windows")]
fn volume_serial(path: &Path) -> Result<u64, WindowsError> {
    crate::native::path::volume_serial(path)
}

fn validate_component(component: &str) -> Result<(), WindowsError> {
    let invalid = component.is_empty()
        || component == "."
        || component == ".."
        || component.ends_with(['.', ' '])
        || component.contains(':')
        || component
            .chars()
            .any(|value| value.is_control() || matches!(value, '<' | '>' | '"' | '|' | '?' | '*'));
    let base = component.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || is_numbered_reserved(&base, "COM")
        || is_numbered_reserved(&base, "LPT");
    if invalid || reserved {
        Err(path_error("path contains traversal, ADS, wildcard, or reserved-name syntax"))
    } else {
        Ok(())
    }
}

fn is_numbered_reserved(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

fn path_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Path,
        WindowsOperation::ResolvePath,
        WindowsRecovery::CorrectRequest,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::WindowsPath;

    #[test]
    fn trusted_canonical_drive_path_removes_only_the_extended_length_prefix() {
        let path = std::path::Path::new(r"\\?\d:\qualification\peritus-helper.exe");
        let normalized = WindowsPath::from_canonicalized(path).expect("canonical drive path");

        assert_eq!(normalized.as_str(), "D:/qualification/peritus-helper.exe");
        assert!(WindowsPath::new(path.to_string_lossy()).is_err());
        assert!(
            WindowsPath::from_canonicalized(std::path::Path::new(r"\\?\UNC\host\share")).is_err()
        );
    }
}
