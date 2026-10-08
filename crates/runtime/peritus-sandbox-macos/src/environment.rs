//! Exact non-secret target environment projection.

use core::fmt;
use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
};

use peritus_process::ExecutionPlan;

use crate::{
    MacosError, MacosOperation,
    canonical::{Reader, Writer},
    error,
};

const MAX_ENVIRONMENT_NAME_BYTES: usize = 255;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;

/// One exact non-secret environment assignment carried in the protected manifest.
#[derive(Clone, Eq, PartialEq)]
pub struct EnvironmentEntry {
    name: OsString,
    value: OsString,
}

impl EnvironmentEntry {
    /// Creates one exact native target assignment.
    ///
    /// # Errors
    /// Rejects empty names, equals or NUL in names, and NUL-bearing values.
    pub fn new(name: OsString, value: OsString) -> Result<Self, MacosError> {
        if name.is_empty()
            || name.as_bytes().contains(&b'=')
            || name.as_bytes().contains(&0)
            || value.as_bytes().contains(&0)
        {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "target environment assignment is invalid",
            ));
        }
        Ok(Self { name, value })
    }

    /// Returns the exact variable name.
    #[must_use]
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Returns the exact non-secret variable value.
    #[must_use]
    pub fn value(&self) -> &OsStr {
        &self.value
    }

    pub(crate) fn encode(&self, writer: &mut Writer) -> Result<(), MacosError> {
        writer.native_bytes(self.name.as_bytes())?;
        writer.native_bytes(self.value.as_bytes())
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        Self::new(
            OsString::from_vec(reader.native_bytes()?.to_vec()),
            OsString::from_vec(reader.native_bytes()?.to_vec()),
        )
    }

    pub(crate) fn decode_legacy(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        let name = reader.string()?;
        let value = reader.string()?;
        let valid_name = !name.is_empty()
            && name.len() <= MAX_ENVIRONMENT_NAME_BYTES
            && name.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            });
        if !valid_name || value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.as_bytes().contains(&0)
        {
            return Err(error::invalid(
                MacosOperation::Manifest,
                "target environment assignment is invalid or excessive",
            ));
        }
        Ok(Self { name: name.into(), value: value.into() })
    }
}

impl fmt::Debug for EnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentEntry")
            .field("name", &self.name)
            .field("value_bytes", &self.value.as_bytes().len())
            .finish()
    }
}

pub(crate) fn project_environment(
    execution: &ExecutionPlan,
) -> Result<Vec<EnvironmentEntry>, MacosError> {
    let mut entries = execution
        .environment()
        .variables()
        .iter()
        .map(|variable| {
            EnvironmentEntry::new(variable.name().to_owned(), variable.value().to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    canonicalize(&mut entries)?;
    Ok(entries)
}

pub(crate) fn canonicalize(entries: &mut [EnvironmentEntry]) -> Result<(), MacosError> {
    entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    if entries
        .windows(2)
        .any(|pair| pair[0].name.as_bytes() == pair[1].name.as_bytes())
    {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target environment contains duplicate names",
        ));
    }
    Ok(())
}
