//! Lossless native path text conversion, display escaping, and ordinal comparison.
#[cfg(target_os = "windows")]
use super::MAX_PATH_UNITS;
use super::path_error;
use crate::WindowsError;
use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    fmt::Write as _,
};

#[cfg(target_os = "windows")]
pub(super) fn native_units(value: &OsStr) -> Result<Vec<u16>, WindowsError> {
    use std::os::windows::ffi::OsStrExt;

    let units: Vec<u16> = value.encode_wide().collect();
    if units.len() >= MAX_PATH_UNITS {
        return Err(path_error("native Windows path exceeds its representation"));
    }
    Ok(units)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn native_units(value: &OsStr) -> Result<Vec<u16>, WindowsError> {
    value
        .to_str()
        .map(|text| text.encode_utf16().collect())
        .ok_or_else(|| path_error("native Windows path is not representable on this host"))
}

#[cfg(target_os = "windows")]
pub(super) fn os_string_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    use std::os::windows::ffi::OsStringExt;

    if units.len() >= MAX_PATH_UNITS {
        return Err(path_error("native Windows path exceeds its representation"));
    }
    Ok(OsString::from_wide(units))
}

#[cfg(not(target_os = "windows"))]
pub(super) fn os_string_from_units(units: &[u16]) -> Result<OsString, WindowsError> {
    String::from_utf16(units)
        .map(Into::into)
        .map_err(|_| path_error("native Windows path is not representable on this host"))
}

#[cfg(target_os = "windows")]
pub(super) fn native_units_cmp(left: &[u16], right: &[u16]) -> Ordering {
    let Ok(left_text) = os_string_from_units(left) else {
        return left.cmp(right);
    };
    let Ok(right_text) = os_string_from_units(right) else {
        return left.cmp(right);
    };
    crate::native::path::compare_names(&left_text, &right_text)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn native_units_cmp(left: &[u16], right: &[u16]) -> Ordering {
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

pub(super) fn display_units(units: &[u16]) -> String {
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
