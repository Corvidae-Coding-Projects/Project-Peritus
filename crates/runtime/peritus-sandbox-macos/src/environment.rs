//! Exact non-secret target environment projection.

use core::fmt;

use peritus_process::ExecutionPlan;

use crate::{
    MacosError, MacosOperation,
    canonical::{Reader, Writer},
    error,
};

const MAX_ENVIRONMENT_NAMES: usize = 1_024;
const MAX_ENVIRONMENT_NAME_BYTES: usize = 255;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;

/// One exact non-secret environment assignment carried in the protected manifest.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentEntry {
    name: String,
    value: String,
}

impl EnvironmentEntry {
    /// Creates one portable target assignment.
    ///
    /// # Errors
    /// Rejects an invalid name, NUL-bearing value, or an excessive value.
    pub fn new(name: String, value: String) -> Result<Self, MacosError> {
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
        Ok(Self { name, value })
    }

    /// Returns the exact variable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact non-secret variable value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn encode(&self, writer: &mut Writer) -> Result<(), MacosError> {
        writer.string(&self.name)?;
        writer.string(&self.value)
    }

    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, MacosError> {
        Self::new(reader.string()?, reader.string()?)
    }
}

impl fmt::Debug for EnvironmentEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentEntry")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
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
    if entries.len() > MAX_ENVIRONMENT_NAMES {
        return Err(error::limited(
            MacosOperation::Manifest,
            "target environment assignment count exceeds its bound",
        ));
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    if entries.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(error::invalid(
            MacosOperation::Manifest,
            "target environment contains duplicate names",
        ));
    }
    Ok(())
}
