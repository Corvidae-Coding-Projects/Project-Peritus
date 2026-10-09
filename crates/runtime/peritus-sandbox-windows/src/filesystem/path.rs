//! Lexical Windows path normalization and native reparse evidence.

use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    fmt::Write as _,
    path::PathBuf,
};

#[cfg(any(target_os = "windows", test))]
use std::path::Path;

use peritus_sandbox::SandboxPath;
use peritus_types::Sha256Digest;

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// Maximum Win32 path buffer, including the terminating NUL unit.
const MAX_PATH_UNITS: usize = 32_767;

/// Canonical drive-absolute Windows path using `/` separators and an uppercase drive.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowsPath {
    canonical: OsString,
    canonical_units: Vec<u16>,
    native: PathBuf,
    display: String,
    digest: Sha256Digest,
}

impl WindowsPath {
    /// Normalizes a drive-absolute Windows path and rejects device/UNC/ADS/reserved syntax.
    ///
    /// # Errors
    /// Rejects non-drive-absolute, traversal, device, UNC, ADS, reserved-name, trailing-dot/space,
    /// wildcard, control, or over-limit representations.
    pub fn new(value: impl AsRef<str>) -> Result<Self, WindowsError> {
        Self::from_os_str(OsStr::new(value.as_ref()))
    }

    /// Normalizes exact native Windows path text without a Unicode projection.
    ///
    /// # Errors
    /// Rejects non-drive-absolute, traversal, device, UNC, ADS, reserved-name, trailing-dot/space,
    /// wildcard, control, NUL, or over-limit representations.
    pub fn from_os_str(value: &OsStr) -> Result<Self, WindowsError> {
        Self::from_units(native_units(value)?)
    }

    /// Converts the trusted drive path returned by `std::fs::canonicalize` into the policy form.
    ///
    /// Windows canonicalization adds the extended-length `\\?\` prefix. That prefix is an OS
    /// representation detail, so native probes remove it before applying the ordinary strict path
    /// policy. UNC and other device paths remain rejected by [`Self::new`].
    ///
    /// # Errors
    /// Rejects non-drive paths or invalid native syntax.
    #[cfg(any(target_os = "windows", test))]
    pub fn from_canonicalized(path: &Path) -> Result<Self, WindowsError> {
        let mut units = native_units(path.as_os_str())?;
        let extended_prefix =
            [u16::from(b'\\'), u16::from(b'\\'), u16::from(b'?'), u16::from(b'\\')];
        if units.starts_with(&extended_prefix) {
            units.drain(..extended_prefix.len());
        }
        Self::from_units(units)
    }

    /// Resolves a logical sandbox path beneath a canonical workspace.
    pub(crate) fn from_sandbox(workspace: &Self, path: &SandboxPath) -> Result<Self, WindowsError> {
        let text = path.as_str();
        if text.len() >= 3 && text.as_bytes()[1] == b':' {
            return Self::new(text);
        }
        let relative =
            text.strip_prefix('/').ok_or_else(|| path_error("logical path is not absolute"))?;
        if relative.is_empty() {
            return Ok(workspace.clone());
        }
        let mut joined = native_units(&workspace.canonical)?;
        if joined.last() != Some(&u16::from(b'/')) {
            joined.push(u16::from(b'/'));
        }
        joined.extend(relative.encode_utf16());
        Self::from_units(joined)
    }

    /// Returns canonical DOS path text for diagnostics.
    ///
    /// Unpaired native UTF-16 units use an explicit `\u{NNNN}` diagnostic escape. Native launch,
    /// comparison, persistence, and digesting use [`Self::as_os_str`] and never this projection.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.display
    }

    /// Returns the canonical DOS path as native text.
    #[must_use]
    pub fn as_os_str(&self) -> &OsStr {
        self.canonical.as_os_str()
    }

    /// Compares paths with Windows native ordinal case-insensitive semantics.
    #[must_use]
    pub fn native_cmp(&self, other: &Self) -> Ordering {
        native_units_cmp(&self.canonical_units, &other.canonical_units)
    }

    /// Orders native identities first and exact spellings second.
    ///
    /// The exact-spelling tie break makes alias rejection deterministic without treating two
    /// case-fold aliases as distinct Windows identities.
    pub(crate) fn stable_native_cmp(&self, other: &Self) -> Ordering {
        self.native_cmp(other).then_with(|| self.canonical_units.cmp(&other.canonical_units))
    }

    /// Reports whether two exact spellings name the same case-insensitive Windows path.
    #[must_use]
    pub fn same_native_path(&self, other: &Self) -> bool {
        self.native_cmp(other).is_eq()
    }

    /// Returns the legacy ASCII-folded diagnostic spelling; use native comparisons for identity.
    #[must_use]
    pub fn case_folded(&self) -> String {
        self.display.to_ascii_lowercase()
    }

    /// Returns the normalized exact-native path digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports exact same-volume containment using component boundaries.
    #[must_use]
    pub fn contains(&self, candidate: &Self) -> bool {
        let base = &self.canonical_units;
        let candidate = &candidate.canonical_units;
        base.len() <= candidate.len()
            && native_units_cmp(base, &candidate[..base.len()]).is_eq()
            && (base.len() == candidate.len()
                || base.last() == Some(&u16::from(b'/'))
                || candidate.get(base.len()) == Some(&u16::from(b'/')))
    }

    /// Returns an OS path using the platform-native separator parser.
    #[must_use]
    pub fn to_path_buf(&self) -> PathBuf {
        self.native.clone()
    }

    /// Returns the direct lexical parent without consulting the host filesystem.
    #[cfg(target_os = "windows")]
    pub(crate) fn parent(&self) -> Result<Option<Self>, WindowsError> {
        if self.canonical_units.len() <= 3 {
            return Ok(None);
        }
        let separator = self
            .canonical_units
            .iter()
            .rposition(|unit| *unit == u16::from(b'/'))
            .ok_or_else(|| path_error("drive-absolute path has no component boundary"))?;
        let end = if separator == 2 { 3 } else { separator };
        Self::from_units(self.canonical_units[..end].to_vec()).map(Some)
    }

    fn from_units(mut units: Vec<u16>) -> Result<Self, WindowsError> {
        for unit in &mut units {
            if *unit == u16::from(b'\\') {
                *unit = u16::from(b'/');
            }
        }
        if units.len() >= MAX_PATH_UNITS
            || units.starts_with(&[u16::from(b'/'), u16::from(b'/')])
            || units.starts_with(&[
                u16::from(b'/'),
                u16::from(b'?'),
                u16::from(b'?'),
                u16::from(b'/'),
            ])
            || units.len() < 3
            || !is_ascii_alpha(units[0])
            || units[1] != u16::from(b':')
            || units[2] != u16::from(b'/')
        {
            return Err(path_error("path is not a bounded drive-absolute DOS path"));
        }
        units[0] = ascii_upper(units[0]);
        while units.len() > 3 && units.last() == Some(&u16::from(b'/')) {
            units.pop();
        }
        let mut normalized = units[..3].to_vec();
        if units.len() > 3 {
            for component in units[3..].split(|unit| *unit == u16::from(b'/')) {
                validate_component(component)?;
                if normalized.len() > 3 {
                    normalized.push(u16::from(b'/'));
                }
                normalized.extend_from_slice(component);
            }
        }
        let canonical = os_string_from_units(&normalized)?;
        let mut native_units = normalized.clone();
        for unit in &mut native_units {
            if *unit == u16::from(b'/') {
                *unit = u16::from(b'\\');
            }
        }
        let native = PathBuf::from(os_string_from_units(&native_units)?);
        let mut digest_input = Vec::with_capacity(normalized.len().saturating_mul(2));
        for unit in &normalized {
            digest_input.extend_from_slice(&unit.to_be_bytes());
        }
        // Preserve V1 identity bytes for all previously representable paths.
        let digest = String::from_utf16(&normalized).map_or_else(
            |_| peritus_codec::sha256(&digest_input),
            |text| peritus_codec::sha256(text.to_ascii_lowercase().as_bytes()),
        );
        let display = display_units(&normalized);
        Ok(Self { canonical, canonical_units: normalized, native, display, digest })
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
        if volume_serial == 0 || !lexical.same_native_path(&resolved) {
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
        let resolved = WindowsPath::from_canonicalized(&canonical)?;
        let evidence = PathEvidence::new(path, resolved, volume_serial(&native)?, true, true)?;
        Self::from_evidence(evidence)
    }

    /// Resolves either the exact target or, when only the final component is absent, its direct
    /// existing parent. The returned boolean reports whether the exact target exists.
    ///
    /// # Errors
    /// Rejects inaccessible targets, missing parents, reparse traversal, or canonical aliases.
    #[cfg(target_os = "windows")]
    pub(crate) fn resolve_existing_or_parent(
        path: WindowsPath,
    ) -> Result<(Self, bool), WindowsError> {
        match std::fs::symlink_metadata(path.to_path_buf()) {
            Ok(_) => Self::resolve(path).map(|resolved| (resolved, true)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                let parent = path.parent()?.ok_or_else(|| {
                    path_error("missing ACL target has no existing authorized parent")
                })?;
                Self::resolve(parent).map(|resolved| (resolved, false))
            }
            Err(_) => Err(path_error("path target cannot be inspected")),
        }
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

fn validate_component(component: &[u16]) -> Result<(), WindowsError> {
    let invalid = component.is_empty()
        || component == [u16::from(b'.')]
        || component == [u16::from(b'.'), u16::from(b'.')]
        || component.last().is_some_and(|unit| matches!(*unit, 0x2e | 0x20))
        || component.contains(&u16::from(b':'))
        || component
            .iter()
            .any(|unit| matches!(*unit, 0..=31 | 0x3c | 0x3e | 0x22 | 0x7c | 0x3f | 0x2a));
    let base = component
        .split(|unit| *unit == u16::from(b'.'))
        .next()
        .unwrap_or_default()
        .iter()
        .copied()
        .map(ascii_upper)
        .collect::<Vec<_>>();
    let reserved = matches_ascii(&base, b"CON")
        || matches_ascii(&base, b"PRN")
        || matches_ascii(&base, b"AUX")
        || matches_ascii(&base, b"NUL")
        || is_numbered_reserved(&base, b"COM")
        || is_numbered_reserved(&base, b"LPT");
    if invalid || reserved {
        Err(path_error("path contains traversal, ADS, wildcard, or reserved-name syntax"))
    } else {
        Ok(())
    }
}

fn is_numbered_reserved(value: &[u16], prefix: &[u8]) -> bool {
    value.len() == prefix.len() + 1
        && matches_ascii(&value[..prefix.len()], prefix)
        && matches!(value[prefix.len()], 0x31..=0x39)
}

fn matches_ascii(value: &[u16], expected: &[u8]) -> bool {
    value.len() == expected.len()
        && value.iter().zip(expected).all(|(left, right)| *left == u16::from(*right))
}

const fn is_ascii_alpha(value: u16) -> bool {
    matches!(value, 0x41..=0x5a | 0x61..=0x7a)
}

const fn ascii_upper(value: u16) -> u16 {
    if matches!(value, 0x61..=0x7a) { value - 0x20 } else { value }
}

#[cfg(target_os = "windows")]
fn native_units(value: &OsStr) -> Result<Vec<u16>, WindowsError> {
    use std::os::windows::ffi::OsStrExt;

    let units: Vec<u16> = value.encode_wide().collect();
    if units.len() >= MAX_PATH_UNITS {
        return Err(path_error("native Windows path exceeds its representation"));
    }
    Ok(units)
}

#[cfg(not(target_os = "windows"))]
fn native_units(value: &OsStr) -> Result<Vec<u16>, WindowsError> {
    value
        .to_str()
        .map(|text| text.encode_utf16().collect())
        .ok_or_else(|| path_error("native Windows path is not representable on this host"))
}

#[cfg(target_os = "windows")]
fn os_string_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    use std::os::windows::ffi::OsStringExt;

    if units.len() >= MAX_PATH_UNITS {
        return Err(path_error("native Windows path exceeds its representation"));
    }
    Ok(OsString::from_wide(units))
}

#[cfg(not(target_os = "windows"))]
fn os_string_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    String::from_utf16(units)
        .map(Into::into)
        .map_err(|_| path_error("native Windows path is not representable on this host"))
}

#[cfg(target_os = "windows")]
fn native_units_cmp(left: &[u16], right: &[u16]) -> Ordering {
    let Ok(left_text) = os_string_from_units(left) else {
        return left.cmp(right);
    };
    let Ok(right_text) = os_string_from_units(right) else {
        return left.cmp(right);
    };
    crate::native::path::compare_names(&left_text, &right_text)
}

#[cfg(not(target_os = "windows"))]
fn native_units_cmp(left: &[u16], right: &[u16]) -> Ordering {
    windows_ordinal_key(left).cmp(&windows_ordinal_key(right))
}

#[cfg(not(target_os = "windows"))]
fn windows_ordinal_key(units: &[u16]) -> Vec<u16> {
    let mut key = Vec::with_capacity(units.len());
    for value in char::decode_utf16(units.iter().copied()) {
        match value {
            Ok(character) => {
                // Ordinal Windows comparisons do not perform multi-character expansions.
                let mut uppercase = character.to_uppercase();
                let first = uppercase.next().unwrap_or(character);
                let folded = if uppercase.next().is_none() { first } else { character };
                let mut encoded = [0_u16; 2];
                key.extend_from_slice(folded.encode_utf16(&mut encoded));
            }
            Err(error) => key.push(error.unpaired_surrogate()),
        }
    }
    key
}

fn display_units(units: &[u16]) -> String {
    let mut display = String::new();
    for value in char::decode_utf16(units.iter().copied()) {
        match value {
            Ok(character) => display.push(character),
            Err(error) => {
                let _ = write!(display, "\\u{{{:04X}}}", error.unpaired_surrogate());
            }
        }
    }
    display
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
