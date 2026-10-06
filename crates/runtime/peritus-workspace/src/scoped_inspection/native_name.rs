//! Lossless native entry identity, separate from display and effect-authorizing paths.

use super::{WorkspaceError, invalid};
use std::ffi::{OsStr, OsString};

/// Encoding of an exact native child name. These bytes are not a portable authority path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeNameEncoding {
    /// Unix bytes, including names outside UTF-8.
    UnixBytes,
    /// Windows UTF-16 code units, stored in big-endian order without replacement characters.
    WindowsWide,
    /// UTF-8 for hosts without Unix or Windows native extensions.
    Utf8,
}

/// One exact native child name with a separately escaped, non-authorizing display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEntryName {
    encoding: NativeNameEncoding,
    bytes: Vec<u8>,
}
impl NativeEntryName {
    pub(super) fn observed(name: &OsStr) -> Result<Self, WorkspaceError> {
        #[cfg(unix)]
        let observed = {
            use std::os::unix::ffi::OsStrExt as _;
            Self { encoding: NativeNameEncoding::UnixBytes, bytes: name.as_bytes().to_vec() }
        };
        #[cfg(windows)]
        let observed = {
            use std::os::windows::ffi::OsStrExt as _;
            Self {
                encoding: NativeNameEncoding::WindowsWide,
                bytes: name.encode_wide().flat_map(u16::to_be_bytes).collect(),
            }
        };
        #[cfg(not(any(unix, windows)))]
        let observed = {
            Self {
                encoding: NativeNameEncoding::Utf8,
                bytes: name
                    .to_str()
                    .ok_or_else(|| invalid("host cannot encode this native name losslessly"))?
                    .as_bytes()
                    .to_vec(),
            }
        };
        observed.native_name()?;
        Ok(observed)
    }
    /// Returns the explicit native encoding.
    #[must_use]
    pub const fn encoding(&self) -> NativeNameEncoding {
        self.encoding
    }
    /// Borrows exact native units; these bytes must never be parsed as a display path.
    #[must_use]
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Reconstructs the exact native child name on its original platform.
    ///
    /// # Errors
    /// Rejects a foreign platform encoding or invalid native child representation.
    pub fn native_name(&self) -> Result<OsString, WorkspaceError> {
        if self.bytes.is_empty() {
            return Err(invalid("native entry name is empty"));
        }
        let name = self.decode()?;
        let mut components = std::path::Path::new(&name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(child)) if child == name)
            || components.next().is_some()
        {
            return Err(invalid("native entry identity is not one relative child"));
        }
        if name.as_encoded_bytes().contains(&0) {
            return Err(invalid("native entry name contains NUL"));
        }
        Ok(name)
    }
    fn decode(&self) -> Result<OsString, WorkspaceError> {
        #[cfg(unix)]
        if self.encoding == NativeNameEncoding::UnixBytes {
            use std::os::unix::ffi::OsStringExt as _;
            return Ok(OsString::from_vec(self.bytes.clone()));
        }
        #[cfg(windows)]
        if self.encoding == NativeNameEncoding::WindowsWide && self.bytes.len().is_multiple_of(2) {
            use std::os::windows::ffi::OsStringExt as _;
            let units = self
                .bytes
                .chunks_exact(2)
                .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                .collect::<Vec<_>>();
            return Ok(OsString::from_wide(&units));
        }
        #[cfg(not(any(unix, windows)))]
        if self.encoding == NativeNameEncoding::Utf8 {
            return String::from_utf8(self.bytes.clone())
                .map(OsString::from)
                .map_err(|_| invalid("native entry name is not UTF-8"));
        }
        Err(invalid("retained native name belongs to another platform"))
    }
    /// Returns escaped display text. Only the native units identify the child.
    #[must_use]
    pub fn display_name(&self) -> String {
        use std::fmt::Write as _;
        if let Ok(name) = self.native_name()
            && let Some(text) = name.to_str()
        {
            return text.escape_debug().collect();
        }
        let mut text = match self.encoding {
            NativeNameEncoding::UnixBytes => "unix:",
            NativeNameEncoding::WindowsWide => "windows-utf16:",
            NativeNameEncoding::Utf8 => "utf8:",
        }
        .to_owned();
        for byte in &self.bytes {
            write!(&mut text, "{byte:02x}").expect("string formatting");
        }
        text
    }
    pub(super) const fn tag(&self) -> u8 {
        match self.encoding {
            NativeNameEncoding::UnixBytes => 1,
            NativeNameEncoding::WindowsWide => 2,
            NativeNameEncoding::Utf8 => 3,
        }
    }
    pub(super) fn from_encoded(tag: u8, bytes: Vec<u8>) -> Result<Self, WorkspaceError> {
        let encoding = match tag {
            1 => NativeNameEncoding::UnixBytes,
            2 => NativeNameEncoding::WindowsWide,
            3 => NativeNameEncoding::Utf8,
            _ => return Err(invalid("unknown native name encoding")),
        };
        let name = Self { encoding, bytes };
        name.native_name()?;
        Ok(name)
    }
    pub(super) fn protected(&self) -> bool {
        let units: Vec<u16> = if self.encoding == NativeNameEncoding::WindowsWide {
            self.bytes
                .chunks_exact(2)
                .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
                .collect()
        } else {
            self.bytes.iter().copied().map(u16::from).collect()
        };
        let matches = |value: &[u8]| {
            units.len() == value.len()
                && units.iter().zip(value).all(|(unit, byte)| {
                    u8::try_from(*unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(byte))
                })
        };
        matches(b".git")
            || matches(b".peritus")
            || units.len() >= 13
                && units[..13].iter().zip(b".peritus-txn-").all(|(unit, byte)| {
                    u8::try_from(*unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(byte))
                })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_names_are_exact_bare_children_without_path_normalization_aliases() {
        for invalid in
            ["", ".", "..", "./valid", "valid/", "valid//", "valid/.", "a/b", "bad\0name"]
        {
            assert!(NativeEntryName::observed(OsStr::new(invalid)).is_err(), "{invalid:?}");
        }
        let valid = NativeEntryName::observed(OsStr::new("valid")).unwrap();
        assert_eq!(valid.native_name().unwrap(), OsStr::new("valid"));
        assert_eq!(
            NativeEntryName::from_encoded(valid.tag(), valid.encoded_bytes().to_vec()).unwrap(),
            valid
        );
    }
}
